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
//! app database never stops the queue from draining. An asked-for flush
//! and stop wait for every one still running.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::jobs::Enqueue;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{Instant, Interval, MissedTickBehavior};

use crate::cache::{CachedSite, Sites};
use crate::files::{ProjectDb, Submit};
use crate::hooks::GoalConverted;
use crate::rollup::{self, Rollup};
use crate::store;
use crate::write::{self, Incoming};

/// Events in one flush before the window is up.
pub(crate) const FLUSH_EVENTS: usize = 1_000;
/// Waits between attempts at a flush's app transaction.
const SIDE_EFFECT_RETRIES: [Duration; 3] =
    [Duration::from_millis(100), Duration::from_millis(500), Duration::from_secs(2)];
/// App transactions in flight at once; the next flush waits for one to end.
const SIDE_EFFECT_TASKS: usize = 4;

type Effects = JoinSet<anyhow::Result<()>>;

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

    /// Asks for a flush now. The future resolves after its side effects.
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
        let mut effects = Effects::new();
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
                Some(done) = effects.join_next(), if !effects.is_empty() => {
                    reaped(done).unwrap_or_default();
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
                    let settled = settle(&mut effects).await;
                    answer.send(result.and(settled)).unwrap_or_default();
                }
                Some(Msg::Stop(answer)) => {
                    self.flush(&mut pending, &mut count, &mut effects).await.unwrap_or_default();
                    settle(&mut effects).await.unwrap_or_default();
                    answer.send(()).unwrap_or_default();
                    return;
                }
                None => break,
            }
        }
        self.flush(&mut pending, &mut count, &mut effects).await.unwrap_or_default();
        settle(&mut effects).await.unwrap_or_default();
    }

    /// Commits `pending` and starts its side effects in `effects`. Failures
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
        if owes_app(&site, &flushed) {
            while effects.len() >= SIDE_EFFECT_TASKS {
                if let Some(done) = effects.join_next().await {
                    reaped(done).unwrap_or_default();
                }
            }
            effects.spawn(side_effects(
                self.platform.clone(),
                Arc::clone(&self.sites),
                site,
                flushed,
            ));
        }
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
    let mut tx = platform.db().write().await?;
    let advanced = match day {
        Some(day) => store::advance_event_day(&mut *tx, workspace_id, project_id, day).await?,
        None => false,
    };
    if advanced {
        let job = Rollup { workspace_id, project_id };
        let opts = Enqueue { run_at: Some(platform.now().plus(rollup::DELAY)) };
        platform.bus().enqueue(&mut tx, &job, opts).await?;
    }
    for converted in conversions {
        platform.bus().publish(&mut tx, converted).await?;
    }
    tx.commit().await?;
    if advanced {
        sites.clear();
    }
    Ok(())
}

/// Waits for every side-effect task and returns the first failure.
async fn settle(effects: &mut Effects) -> anyhow::Result<()> {
    let mut first_err = None;
    while let Some(done) = effects.join_next().await {
        if let Err(err) = reaped(done) {
            first_err.get_or_insert(err);
        }
    }
    first_err.map_or(Ok(()), Err)
}

/// A finished side-effect task's result. The task logged its own failure.
fn reaped(done: Result<anyhow::Result<()>, tokio::task::JoinError>) -> anyhow::Result<()> {
    done.unwrap_or_else(|err| {
        tracing::error!(error = %err, "an analytics side-effect task panicked");
        Err(anyhow::anyhow!("a side-effect task panicked: {err}"))
    })
}

async fn next_tick(tick: Option<&mut Interval>) {
    match tick {
        Some(tick) => {
            tick.tick().await;
        }
        None => std::future::pending().await,
    }
}
