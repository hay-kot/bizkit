//! The buffered writer: one task per project file. Ingest queues a
//! request's events and answers at once; the task commits what is queued
//! every flush window, or as soon as [`FLUSH_EVENTS`] are waiting, in one
//! transaction. Then it writes what the flush owes the app database (the
//! event day, the rollup job, goal hook events) in one app transaction.
//!
//! The project commit comes first because the hook events carry the event
//! rowid. A crash between the two loses that flush's hook runs and day
//! advance; the next flush advances the day again. A crash loses at most
//! one window of queued events; a clean shutdown loses none.
//!
//! The app transaction runs in its own task, with its retries, so a busy
//! app database never stops the queue from draining. One runs at a time,
//! in flush order; what later flushes owe meanwhile merges into the next
//! one. An asked-for flush is answered once the side effects it owes are
//! done, while the loop goes on draining; stop waits until nothing is owed.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::jobs::Enqueue;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Instant, Interval, MissedTickBehavior};

use crate::cache::{CachedSite, Sites};
use crate::files::{ProjectDb, Submit};
use crate::hooks::GoalConverted;
use crate::rollup::{self, Rollup};
use crate::store;
use crate::write::{self, Incoming};

/// Events in one flush before the window is up.
pub(crate) const FLUSH_EVENTS: usize = 1_000;
/// Conversions owed to the app database while it is busy; past this the
/// newest are dropped with an error log, so an outage can't grow memory.
const MAX_OWED_CONVERSIONS: usize = 10_000;
/// Waits between attempts at a flush's app transaction.
const SIDE_EFFECT_RETRIES: [Duration; 3] =
    [Duration::from_millis(100), Duration::from_millis(500), Duration::from_secs(2)];

/// One request's events and the queue room they hold until they commit.
#[derive(Debug)]
struct Queued {
    site: Arc<CachedSite>,
    events: Vec<Incoming>,
    _room: (OwnedSemaphorePermit, OwnedSemaphorePermit),
}

#[derive(Debug)]
enum Msg {
    Events(Queued),
    Flush(oneshot::Sender<anyhow::Result<()>>),
    Stop(oneshot::Sender<()>),
}

/// A running writer.
#[derive(Debug)]
pub(crate) struct WriterHandle {
    tx: mpsc::UnboundedSender<Msg>,
    task: JoinHandle<()>,
    room: Arc<Semaphore>,
}

/// Starts the writer for one file. `flush` is the window; `None` flushes
/// only on size, [`WriterHandle::flush`] and stop.
pub(crate) fn start(
    platform: Platform,
    sites: Arc<Sites>,
    db: Arc<ProjectDb>,
    flush: Option<Duration>,
    queue: usize,
) -> WriterHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let writer = Writer { platform, sites, db };
    let task = tokio::spawn(writer.run(rx, flush));
    WriterHandle { tx, task, room: Arc::new(Semaphore::new(queue)) }
}

impl WriterHandle {
    /// Queues events if this project and the whole process have room.
    pub(crate) fn submit(
        &self,
        site: Arc<CachedSite>,
        events: Vec<Incoming>,
        global: &Arc<Semaphore>,
    ) -> Result<(), Submit> {
        let count = u32::try_from(events.len()).map_err(|_| Submit::Full)?;
        let room = (
            Arc::clone(&self.room).try_acquire_many_owned(count).map_err(|_| Submit::Full)?,
            Arc::clone(global).try_acquire_many_owned(count).map_err(|_| Submit::Full)?,
        );
        let queued = Queued { site, events, _room: room };
        self.tx
            .send(Msg::Events(queued))
            .map_err(|_| Submit::Failed(anyhow::anyhow!("the writer stopped")))
    }

    /// Asks for a flush now. The future resolves after its side effects and
    /// fails only when the events failed to store; side-effect failures are
    /// logged.
    pub(crate) fn flush(&self) -> impl Future<Output = anyhow::Result<()>> + use<> {
        let (answer, done) = oneshot::channel();
        let sent = self.tx.send(Msg::Flush(answer));
        async move {
            sent.map_err(|_| anyhow::anyhow!("the writer stopped"))?;
            done.await.context("the writer stopped mid-flush")?
        }
    }

    /// Flushes what is queued and ends the task.
    pub(crate) async fn stop(self) {
        let (answer, done) = oneshot::channel();
        if self.tx.send(Msg::Stop(answer)).is_ok() {
            done.await.unwrap_or_default();
        }
        if let Err(err) = self.task.await {
            tracing::error!(error = %err, "an analytics writer panicked");
        }
    }
}

struct Writer {
    platform: Platform,
    sites: Arc<Sites>,
    db: Arc<ProjectDb>,
}

/// What a flush committed to the project file and owes the app database.
#[derive(Debug, Default)]
struct Flushed {
    latest_day: Option<String>,
    conversions: Vec<GoalConverted>,
}

impl Writer {
    async fn run(self, mut rx: mpsc::UnboundedReceiver<Msg>, flush: Option<Duration>) {
        let mut pending: Vec<Queued> = Vec::new();
        let mut count = 0;
        let mut effects = Effects::default();
        // `interval` would tick at once; the first flush is one window out.
        let mut tick = flush.map(|window| {
            let mut tick = tokio::time::interval_at(Instant::now() + window, window);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            tick
        });
        loop {
            let msg = tokio::select! {
                msg = rx.recv() => msg,
                () = next_tick(tick.as_mut()) => {
                    self.flush(&mut pending, &mut count, &mut effects).await.unwrap_or_default();
                    continue;
                }
                () = effects.finished() => {
                    effects.start(&self.platform, &self.sites);
                    effects.answer_ready();
                    continue;
                }
            };
            match msg {
                Some(Msg::Events(queued)) => {
                    count += queued.events.len();
                    pending.push(queued);
                    if count >= FLUSH_EVENTS {
                        self.flush(&mut pending, &mut count, &mut effects)
                            .await
                            .unwrap_or_default();
                        if let Some(tick) = tick.as_mut() {
                            tick.reset();
                        }
                    }
                }
                Some(Msg::Flush(answer)) => {
                    let result = self.flush(&mut pending, &mut count, &mut effects).await;
                    effects.answer_when_done(answer, result);
                }
                Some(Msg::Stop(answer)) => {
                    self.flush(&mut pending, &mut count, &mut effects).await.unwrap_or_default();
                    effects.settle(&self.platform, &self.sites).await;
                    effects.answer_ready();
                    answer.send(()).unwrap_or_default();
                    return;
                }
                None => break,
            }
        }
        self.flush(&mut pending, &mut count, &mut effects).await.unwrap_or_default();
        effects.settle(&self.platform, &self.sites).await;
        effects.answer_ready();
    }

    /// Commits `pending` and owes its side effects to `effects`. Failures
    /// are logged here; the result is for callers that asked for the flush.
    async fn flush(
        &self,
        pending: &mut Vec<Queued>,
        count: &mut usize,
        effects: &mut Effects,
    ) -> anyhow::Result<()> {
        let batch = std::mem::take(pending);
        *count = 0;
        let Some(last) = batch.last() else { return Ok(()) };
        let site = Arc::clone(&last.site);
        let flushed = match self.write(&batch).await {
            Ok(flushed) => Ok(flushed),
            Err(err) => {
                tracing::warn!(
                    error = format!("{err:#}"),
                    "an analytics flush failed; writing its requests one at a time"
                );
                self.write_each(&batch).await
            }
        };
        let (flushed, result) = match flushed {
            Ok(flushed) => (flushed, Ok(())),
            Err((flushed, err)) => (flushed, Err(err)),
        };
        effects.owe(site, flushed);
        effects.start(&self.platform, &self.sites);
        result
    }

    /// Writes requests one transaction each, so one bad request loses only
    /// its own events. Returns what the good ones committed, and the first
    /// failure.
    async fn write_each(&self, batch: &[Queued]) -> Result<Flushed, (Flushed, anyhow::Error)> {
        let mut total = Flushed::default();
        let mut first_err = None;
        for queued in batch {
            match self.write(std::slice::from_ref(queued)).await {
                Ok(flushed) => {
                    total.latest_day = total.latest_day.max(flushed.latest_day);
                    total.conversions.extend(flushed.conversions);
                }
                Err(err) => {
                    tracing::error!(
                        error = format!("{err:#}"),
                        project_id = %queued.site.row.project_id,
                        events = queued.events.len(),
                        "dropped analytics events that failed to store"
                    );
                    first_err.get_or_insert(err);
                }
            }
        }
        match first_err {
            None => Ok(total),
            Some(err) => Err((total, err)),
        }
    }

    /// Writes the requests in queue order in one project transaction, so an
    /// identify sees the events queued before it.
    async fn write(&self, batch: &[Queued]) -> anyhow::Result<Flushed> {
        let mut flushed = Flushed::default();
        let mut tx = self.db.write().await?;
        for queued in batch {
            for event in &queued.events {
                write::record_one(&mut tx, &queued.site, event, &mut flushed.conversions).await?;
                if flushed.latest_day.as_deref() < Some(event.day.as_str()) {
                    flushed.latest_day = Some(event.day.clone());
                }
            }
        }
        tx.commit().await.context("committing an analytics flush")?;
        Ok(flushed)
    }
}

/// The new latest day, when it is past the site's.
fn new_day<'a>(site: &CachedSite, flushed: &'a Flushed) -> Option<&'a str> {
    flushed.latest_day.as_deref().filter(|day| site.row.last_event_day.as_deref() < Some(*day))
}

/// Whether the flush needs an app transaction. It doesn't when the day is
/// unchanged and nothing converted, which is the usual case.
fn owes_app(site: &CachedSite, flushed: &Flushed) -> bool {
    new_day(site, flushed).is_some() || !flushed.conversions.is_empty()
}

/// Writes a flush's app side effects, retrying a few times.
async fn side_effects(
    platform: Platform,
    sites: Arc<Sites>,
    site: Arc<CachedSite>,
    flushed: Flushed,
) -> anyhow::Result<()> {
    let day = new_day(&site, &flushed);
    let mut attempt = 0;
    loop {
        match app_write(&platform, &sites, &site, day, &flushed.conversions).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                let Some(wait) = SIDE_EFFECT_RETRIES.get(attempt) else {
                    tracing::error!(
                        error = format!("{err:#}"),
                        project_id = %site.row.project_id,
                        conversions = flushed.conversions.len(),
                        "lost an analytics flush's goal hooks and day advance"
                    );
                    return Err(err);
                };
                attempt += 1;
                tokio::time::sleep(*wait).await;
            }
        }
    }
}

async fn app_write(
    platform: &Platform,
    sites: &Sites,
    site: &CachedSite,
    day: Option<&str>,
    conversions: &[GoalConverted],
) -> anyhow::Result<()> {
    let (workspace_id, project_id) = (site.row.workspace_id, site.row.project_id);
    let mut tx = platform.db().write().await.context("starting the app write")?;
    let advanced = match day {
        Some(day) => store::advance_event_day(&mut *tx, workspace_id, project_id, day)
            .await
            .context("advancing the site's event day")?,
        None => false,
    };
    if advanced {
        let job = Rollup { workspace_id, project_id };
        let opts = Enqueue { run_at: Some(platform.now().plus(rollup::DELAY)) };
        platform.bus().enqueue(&mut tx, &job, opts).await.context("queuing the rollup")?;
    }
    for converted in conversions {
        platform.bus().publish(&mut tx, converted).await.context("publishing a goal conversion")?;
    }
    tx.commit().await.context("committing the app write")?;
    if advanced {
        sites.clear();
    }
    Ok(())
}

/// A writer's side effects: at most one app transaction running, and what
/// flushes owed since it started, merged. Flushes that owe something are
/// numbered, so an asked-for flush is answered when its number is done,
/// even while later flushes keep owing more.
#[derive(Debug, Default)]
struct Effects {
    running: Option<JoinHandle<anyhow::Result<()>>>,
    owed: Option<(Arc<CachedSite>, Flushed)>,
    /// The last flush that owed something.
    owed_through: u64,
    /// The last flush the running transaction covers.
    running_through: u64,
    /// The last flush whose side effects are done.
    done_through: u64,
    /// Asked-for flushes: the flush they wait for, the answer and its result.
    waiting: Vec<(u64, Answer, anyhow::Result<()>)>,
}

type Answer = oneshot::Sender<anyhow::Result<()>>;

impl Effects {
    fn owe(&mut self, site: Arc<CachedSite>, flushed: Flushed) {
        if flushed.latest_day.is_none() && flushed.conversions.is_empty() {
            return;
        }
        self.owed_through += 1;
        let (_, owed) = self.owed.get_or_insert_with(|| (Arc::clone(&site), Flushed::default()));
        owed.latest_day = owed.latest_day.take().max(flushed.latest_day);
        let room = MAX_OWED_CONVERSIONS.saturating_sub(owed.conversions.len());
        let dropped = flushed.conversions.len().saturating_sub(room);
        owed.conversions.extend(flushed.conversions.into_iter().take(room));
        if dropped > 0 {
            tracing::error!(
                project_id = %site.row.project_id,
                dropped,
                "dropped goal hook events owed to a busy app database"
            );
        }
        if let Some((owed_site, _)) = self.owed.as_mut() {
            *owed_site = site;
        }
    }

    /// Starts what is owed unless a transaction is already running.
    fn start(&mut self, platform: &Platform, sites: &Arc<Sites>) {
        if self.running.is_some() {
            return;
        }
        let Some((site, flushed)) = self.owed.take() else { return };
        if owes_app(&site, &flushed) {
            let task = side_effects(platform.clone(), Arc::clone(sites), site, flushed);
            self.running = Some(tokio::spawn(task));
            self.running_through = self.owed_through;
        } else {
            self.done_through = self.owed_through;
        }
    }

    /// Answers now when everything owed so far is done, else when it is.
    fn answer_when_done(&mut self, answer: Answer, result: anyhow::Result<()>) {
        if self.owed_through <= self.done_through {
            answer.send(result).unwrap_or_default();
        } else {
            self.waiting.push((self.owed_through, answer, result));
        }
    }

    /// Answers the asked-for flushes whose side effects are done.
    fn answer_ready(&mut self) {
        let done = self.done_through;
        let (ready, waiting) =
            std::mem::take(&mut self.waiting).into_iter().partition(|(flush, ..)| *flush <= done);
        self.waiting = waiting;
        for (_, answer, result) in ready {
            answer.send(result).unwrap_or_default();
        }
    }

    /// Resolves when the running transaction ends, never when none runs.
    /// The task logged its own failure.
    async fn finished(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return std::future::pending().await;
        };
        let ended = running.await;
        self.running = None;
        self.done_through = self.running_through;
        if let Err(err) = ended {
            tracing::error!(error = %err, "an analytics side-effect task panicked");
        }
    }

    /// Runs everything owed to the end.
    async fn settle(&mut self, platform: &Platform, sites: &Arc<Sites>) {
        loop {
            self.start(platform, sites);
            if self.running.is_none() {
                return;
            }
            self.finished().await;
        }
    }
}

async fn next_tick(tick: Option<&mut Interval>) {
    match tick {
        Some(tick) => {
            tick.tick().await;
        }
        None => std::future::pending().await,
    }
}
