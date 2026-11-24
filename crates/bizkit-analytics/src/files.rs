//! One SQLite file per project for events, rollups, people, aliases and
//! conversions, at `data_dir/analytics/<project_id>.db`. Sites, goals,
//! funnels and salts stay in the app database.
//!
//! Inside the server, every write to a file goes through its single write
//! connection: the buffered writer, the rollup and goal cleanup take turns.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::error::ApiError;
use bizkit_core::ids::ProjectId;
use sqlx::pool::PoolOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};
use tokio::sync::{Mutex, OnceCell, Semaphore};
use tokio::time::Instant;

use crate::cache::{CachedSite, Sites};
use crate::report::Scope;
use crate::store;
use crate::write::Incoming;
use crate::writer::{self, WriterHandle};

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
/// A rollup day or a large flush can hold the one write connection for a
/// few seconds; a writer waiting on it should outlast that, not fail.
const WRITE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);
/// Without a limit the WAL stays at its high-water mark after a long read
/// held back checkpoints.
const JOURNAL_SIZE_LIMIT: &str = "4194304";
const READERS: u32 = 2;
/// Idle projects give their read connections back.
const READER_IDLE: Duration = Duration::from_mins(5);
/// A file unused this long is closed, writer and pools, until its next use.
const FILE_IDLE: Duration = Duration::from_mins(10);
/// How often `Files::get` looks for idle files.
const SWEEP_EVERY: Duration = Duration::from_mins(1);

/// The project-file migrations. Versions share one `_sqlx_migrations`
/// table with the app migrations in the sqlx prepare database, so they
/// must stay unique across both sets.
#[must_use]
pub fn project_migrator() -> sqlx::migrate::Migrator {
    sqlx::migrate!("./migrations-project")
}

/// Where a project's analytics file lives.
#[must_use]
pub fn project_db_path(data_dir: &Path, project_id: ProjectId) -> PathBuf {
    data_dir.join("analytics").join(format!("{project_id}.db"))
}

/// A project file's pools. Not a core `Db`, so a job can never be written
/// into a file that has no `jobs` table.
#[derive(Debug)]
pub(crate) struct ProjectDb {
    read: SqlitePool,
    write: SqlitePool,
}

impl ProjectDb {
    /// Opens (creating if needed) the file at `path` and runs the project
    /// migrations.
    pub(crate) async fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let options = SqliteConnectOptions::from_str("sqlite:")
            .context("building connect options")?
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(BUSY_TIMEOUT)
            .pragma("journal_size_limit", JOURNAL_SIZE_LIMIT);
        // The write pool opens first so it creates the file and switches it
        // to WAL before readers connect.
        let write = PoolOptions::<Sqlite>::new()
            .max_connections(1)
            .acquire_timeout(WRITE_ACQUIRE_TIMEOUT)
            .connect_with(options.clone())
            .await
            .with_context(|| format!("opening {}", path.display()))?;
        project_migrator()
            .run(&write)
            .await
            .with_context(|| format!("migrating {}", path.display()))?;
        let read = PoolOptions::<Sqlite>::new()
            .max_connections(READERS)
            .min_connections(0)
            .idle_timeout(READER_IDLE)
            .connect_with(options.read_only(true))
            .await
            .with_context(|| format!("opening {} for reading", path.display()))?;
        Ok(Self { read, write })
    }

    /// The read pool.
    pub(crate) const fn read(&self) -> &SqlitePool {
        &self.read
    }

    /// The write pool, for helpers that hand a pool to tests.
    pub(crate) fn write_pool(&self) -> SqlitePool {
        self.write.clone()
    }

    /// Starts a write transaction with `BEGIN IMMEDIATE`, as core's
    /// `Db::write` does.
    pub(crate) async fn write(&self) -> anyhow::Result<Transaction<'static, Sqlite>> {
        self.write.begin_with("BEGIN IMMEDIATE").await.context("starting a project write")
    }

    pub(crate) async fn close(&self) {
        self.write.close().await;
        self.read.close().await;
    }
}

/// The writer's state for one file.
#[derive(Debug)]
enum WriterState {
    Idle,
    Running(WriterHandle),
    /// The file was removed or the app is stopping; nothing more is queued.
    Stopped,
}

/// An open project file and its writer, started on the first event.
#[derive(Debug)]
pub(crate) struct ProjectFile {
    pub(crate) db: Arc<ProjectDb>,
    writer: Mutex<WriterState>,
}

impl ProjectFile {
    /// Writes what the project's writer has queued, side effects included.
    pub(crate) async fn flush(&self) -> anyhow::Result<()> {
        let flushed = match &*self.writer.lock().await {
            WriterState::Running(handle) => Some(handle.flush()),
            WriterState::Idle | WriterState::Stopped => None,
        };
        match flushed {
            Some(flushed) => flushed.await,
            None => Ok(()),
        }
    }

    /// Flushes and stops the writer, then closes the pools. Later submits fail.
    async fn stop(&self) {
        let state = std::mem::replace(&mut *self.writer.lock().await, WriterState::Stopped);
        if let WriterState::Running(handle) = state {
            handle.stop().await;
        }
        self.db.close().await;
    }
}

/// Queue bounds and flush timing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    /// Events queued per project before ingest refuses.
    pub(crate) project_queue: usize,
    /// Events queued across projects before ingest refuses.
    pub(crate) global_queue: usize,
    /// Whether writers flush on a timer; tests turn it off and flush by hand.
    pub(crate) timer: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self { project_queue: 10_000, global_queue: 50_000, timer: true }
    }
}

/// Why events weren't queued.
#[derive(Debug)]
pub(crate) enum Submit {
    /// The project's or the global queue is full.
    Full,
    /// The file couldn't open, or the project is gone.
    Failed(anyhow::Error),
}

/// A project's place in [`Files`]: its file once opened, and when it was
/// last asked for.
#[derive(Debug, Default)]
struct Slot {
    file: OnceCell<Arc<ProjectFile>>,
    /// Milliseconds after [`Files::epoch`].
    used: AtomicU64,
    /// Set by `stop_all` and `remove` once the slot left the map. A get that
    /// opened the file anyway stops it, so no writer outlives them.
    closed: AtomicBool,
}

/// Why `get` has no file for a project.
enum Unopened {
    NoSite,
    Failed(anyhow::Error),
}

/// Held by a file's stop task until the file is closed. Whoever needs that
/// project's file waits on it, so two writers never share one file.
type Gate = Arc<Mutex<()>>;

/// What [`Files`] guards with its one lock.
#[derive(Debug, Default)]
struct Open {
    slots: HashMap<ProjectId, Arc<Slot>>,
    /// Files a sweep is closing.
    closing: HashMap<ProjectId, Gate>,
    /// Set by `stop_all`: nothing opens after it.
    stopped: bool,
}

/// Every open project file. A file nobody has asked for in [`FILE_IDLE`]
/// is closed by the next sweep and opens again on its next use.
#[derive(Debug)]
pub(crate) struct Files {
    open: Mutex<Open>,
    sites: Arc<Sites>,
    flush: Duration,
    limits: Limits,
    global: Arc<Semaphore>,
    epoch: Instant,
    /// Milliseconds after `epoch` of the last sweep.
    swept: AtomicU64,
}

impl Files {
    pub(crate) fn new(sites: Arc<Sites>, flush: Duration, limits: Limits) -> Self {
        Self {
            open: Mutex::new(Open::default()),
            sites,
            flush,
            limits,
            global: Arc::new(Semaphore::new(limits.global_queue)),
            epoch: Instant::now(),
            swept: AtomicU64::new(0),
        }
    }

    fn clock(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// The project's file, opening (and creating) it when the project has
    /// a site. `None` when it has none, so a deleted project's cached site
    /// can't bring its file back.
    ///
    /// The map lock covers only finding the slot. Checking the site and
    /// opening the file (with its migrations) run on the project's own
    /// cell, so a slow open holds up only that project. A file a sweep is
    /// closing is waited for first.
    pub(crate) async fn get(
        &self,
        platform: &Platform,
        scope: Scope,
    ) -> anyhow::Result<Option<Arc<ProjectFile>>> {
        let project_id = scope.project_id;
        let slot = loop {
            let mut open = self.open.lock().await;
            anyhow::ensure!(!open.stopped, "analytics is stopping");
            if let Some(gate) = open.closing.get(&project_id).cloned() {
                if gate.try_lock().is_err() {
                    drop(open);
                    drop(gate.lock().await);
                    continue;
                }
                open.closing.remove(&project_id);
            }
            let now = self.clock();
            let slot = Arc::clone(open.slots.entry(project_id).or_default());
            slot.used.store(now, Ordering::Relaxed);
            self.sweep(&mut open, now).await;
            drop(open);
            break slot;
        };
        let opened = slot.file.get_or_try_init(|| open_file(platform, scope)).await.cloned();
        match opened {
            Ok(file) if slot.closed.load(Ordering::SeqCst) => {
                file.stop().await;
                Ok(None)
            }
            Ok(file) => Ok(Some(file)),
            Err(unopened) => {
                self.forget(project_id, &slot).await;
                match unopened {
                    Unopened::NoSite => Ok(None),
                    Unopened::Failed(err) => Err(err),
                }
            }
        }
    }

    /// At most once per [`SWEEP_EVERY`], moves the idle files to `closing`
    /// and stops each in its own task, so the caller doesn't wait for them.
    ///
    /// The stop is quick: after [`FILE_IDLE`] with no use, the timer has
    /// written every queued event long ago, and side-effect retries end
    /// within seconds, so the writer has nothing left for the app database.
    async fn sweep(&self, open: &mut Open, now: u64) {
        let last = self.swept.load(Ordering::Relaxed);
        if now.saturating_sub(last) < millis(SWEEP_EVERY) {
            return;
        }
        self.swept.store(now, Ordering::Relaxed);
        open.closing.retain(|_, gate| gate.try_lock().is_err());
        for (project_id, file) in take_idle(&mut open.slots, now) {
            let gate = Gate::default();
            let closed = Arc::clone(&gate).lock_owned().await;
            open.closing.insert(project_id, gate);
            tokio::spawn(async move {
                file.stop().await;
                drop(closed);
            });
        }
    }

    /// Drops a slot whose open failed or found no site, unless another
    /// caller is still waiting on it and may open it yet.
    async fn forget(&self, project_id: ProjectId, slot: &Arc<Slot>) {
        let mut open = self.open.lock().await;
        let ours = open.slots.get(&project_id).is_some_and(|held| Arc::ptr_eq(held, slot));
        // The map and this caller: nobody else can still fill the cell.
        if ours && slot.file.get().is_none() && Arc::strong_count(slot) == 2 {
            open.slots.remove(&project_id);
        }
    }

    /// The project's file for an operation that already checked the site;
    /// a site deleted in between is a 404.
    pub(crate) async fn for_op(
        &self,
        platform: &Platform,
        scope: Scope,
    ) -> Result<Arc<ProjectFile>, ApiError> {
        self.get(platform, scope).await?.ok_or(ApiError::NotFound)
    }

    /// Queues one request's events for the site's project, starting its
    /// writer on first use.
    pub(crate) async fn submit(
        &self,
        platform: &Platform,
        site: Arc<CachedSite>,
        events: Vec<Incoming>,
    ) -> Result<(), Submit> {
        let scope = Scope { workspace_id: site.row.workspace_id, project_id: site.row.project_id };
        let file = match self.get(platform, scope).await {
            Ok(Some(file)) => file,
            Ok(None) => return Err(Submit::Failed(anyhow::anyhow!("the site is gone"))),
            Err(err) => return Err(Submit::Failed(err)),
        };
        let mut state = file.writer.lock().await;
        if matches!(*state, WriterState::Idle) {
            let flush = self.limits.timer.then_some(self.flush);
            *state = WriterState::Running(writer::start(
                platform.clone(),
                Arc::clone(&self.sites),
                Arc::clone(&file.db),
                flush,
                self.limits.project_queue,
            ));
        }
        let WriterState::Running(handle) = &*state else {
            return Err(Submit::Failed(anyhow::anyhow!("the project file is closed")));
        };
        let submitted = handle.submit(site, events, &self.global);
        drop(state);
        submitted
    }

    /// Writes everything queued in every file, side effects included, and
    /// waits for files being opened or closed.
    pub(crate) async fn flush_all(&self) -> anyhow::Result<()> {
        let (slots, gates) = {
            let open = self.open.lock().await;
            let slots: Vec<Arc<Slot>> = open.slots.values().cloned().collect();
            (slots, open.closing.values().cloned().collect::<Vec<Gate>>())
        };
        let mut first_err = None;
        for slot in slots {
            if let Some(file) = opened(&slot).await
                && let Err(err) = file.flush().await
            {
                first_err.get_or_insert(err);
            }
        }
        for gate in gates {
            drop(gate.lock().await);
        }
        first_err.map_or(Ok(()), Err)
    }

    /// Stops the project's writer after a last flush, closes the file and
    /// deletes it with its WAL and shared-memory files.
    pub(crate) async fn remove(
        &self,
        data_dir: &Path,
        project_id: ProjectId,
    ) -> anyhow::Result<()> {
        let (slot, gate) = {
            let mut open = self.open.lock().await;
            (open.slots.remove(&project_id), open.closing.get(&project_id).cloned())
        };
        if let Some(gate) = gate {
            drop(gate.lock().await);
        }
        if let Some(slot) = slot {
            slot.closed.store(true, Ordering::SeqCst);
            if let Some(file) = opened(&slot).await {
                file.stop().await;
            }
        }
        let path = project_db_path(data_dir, project_id);
        for suffix in ["", "-wal", "-shm"] {
            let mut name = path.clone().into_os_string();
            name.push(suffix);
            match std::fs::remove_file(&name) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(err)
                        .with_context(|| format!("deleting {}", Path::new(&name).display()));
                }
            }
        }
        self.sites.clear();
        Ok(())
    }

    /// Flushes and stops every writer and closes every file, including
    /// files being opened or closed. Later gets fail.
    pub(crate) async fn stop_all(&self) {
        let (slots, gates) = {
            let mut open = self.open.lock().await;
            open.stopped = true;
            let slots: Vec<Arc<Slot>> = open.slots.drain().map(|(_, slot)| slot).collect();
            (slots, open.closing.drain().map(|(_, gate)| gate).collect::<Vec<Gate>>())
        };
        for slot in &slots {
            slot.closed.store(true, Ordering::SeqCst);
        }
        for slot in slots {
            if let Some(file) = opened(&slot).await {
                file.stop().await;
            }
        }
        for gate in gates {
            drop(gate.lock().await);
        }
    }
}

/// The slot's file, after waiting for an open in progress. Opens nothing
/// itself: the init fails at once when no open is running.
async fn opened(slot: &Slot) -> Option<Arc<ProjectFile>> {
    slot.file.get_or_try_init(|| async { Err::<_, Unopened>(Unopened::NoSite) }).await.ok().cloned()
}

/// Checks for the project's site, then opens its file.
async fn open_file(platform: &Platform, scope: Scope) -> Result<Arc<ProjectFile>, Unopened> {
    let site = store::find_site(platform.db().read(), scope.workspace_id, scope.project_id)
        .await
        .context("checking for a site")
        .map_err(Unopened::Failed)?;
    if site.is_none() {
        return Err(Unopened::NoSite);
    }
    let data_dir = platform
        .config()
        .data_dir
        .as_deref()
        .context("data_dir is not set")
        .map_err(Unopened::Failed)?;
    let db = ProjectDb::open(&project_db_path(data_dir, scope.project_id))
        .await
        .map_err(Unopened::Failed)?;
    Ok(Arc::new(ProjectFile { db: Arc::new(db), writer: Mutex::new(WriterState::Idle) }))
}

/// Removes the slots nobody else holds whose files were last used
/// [`FILE_IDLE`] or more before `now`, and returns their files to stop.
/// Callers hold the map lock, and new holders clone only under it, so a
/// count of one can't change before the slot is gone.
fn take_idle(
    slots: &mut HashMap<ProjectId, Arc<Slot>>,
    now: u64,
) -> Vec<(ProjectId, Arc<ProjectFile>)> {
    let mut idle = Vec::new();
    slots.retain(|project_id, slot| {
        let unused = now.saturating_sub(slot.used.load(Ordering::Relaxed)) >= millis(FILE_IDLE);
        if !unused || Arc::strong_count(slot) > 1 {
            return true;
        }
        match slot.file.get() {
            Some(file) if Arc::strong_count(file) > 1 => true,
            Some(file) => {
                idle.push((*project_id, Arc::clone(file)));
                false
            }
            None => false,
        }
    });
    idle
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn file(dir: &Path, name: &str) -> Arc<ProjectFile> {
        let db = ProjectDb::open(&dir.join(format!("{name}.db"))).await.unwrap();
        Arc::new(ProjectFile { db: Arc::new(db), writer: Mutex::new(WriterState::Idle) })
    }

    fn slot(file: Option<Arc<ProjectFile>>, used: u64) -> Arc<Slot> {
        Arc::new(Slot {
            file: OnceCell::new_with(file),
            used: AtomicU64::new(used),
            closed: AtomicBool::new(false),
        })
    }

    fn files() -> Files {
        Files::new(Arc::default(), Duration::from_millis(100), Limits::default())
    }

    #[tokio::test]
    async fn only_idle_unshared_files_are_taken() {
        let dir = tempfile::tempdir().unwrap();
        let now = millis(FILE_IDLE) + 1_000;
        let [idle, recent, held, waited, empty] = std::array::from_fn(|_| ProjectId::new());
        let held_file = file(dir.path(), "held").await;
        let waited_slot = slot(Some(file(dir.path(), "waited").await), 0);
        let mut slots = HashMap::from([
            (idle, slot(Some(file(dir.path(), "idle").await), 1_000)),
            (recent, slot(Some(file(dir.path(), "recent").await), 1_001)),
            (held, slot(Some(Arc::clone(&held_file)), 0)),
            (waited, Arc::clone(&waited_slot)),
            (empty, slot(None, 0)),
        ]);

        let taken = take_idle(&mut slots, now);

        assert_eq!(taken.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [idle], "the idle file");
        assert!(!slots.contains_key(&idle), "the idle file's slot is gone");
        assert!(slots.contains_key(&recent), "used within FILE_IDLE");
        assert!(slots.contains_key(&held), "an operation still holds the file");
        assert!(slots.contains_key(&waited), "a get still holds the slot");
        assert!(!slots.contains_key(&empty), "a slot that never opened is dropped");
        for (_, file) in taken {
            file.stop().await;
        }
        held_file.stop().await;
        for slot in slots.into_values() {
            if let Some(file) = slot.file.get() {
                file.stop().await;
            }
        }
    }

    #[tokio::test]
    async fn sweeps_at_most_once_per_interval() {
        let files = files();
        let mut open = Open::default();
        open.slots.insert(ProjectId::new(), slot(None, 0));
        // Slots last used at 0 are idle from FILE_IDLE on.
        let first = millis(FILE_IDLE);
        files.sweep(&mut open, first).await;
        assert!(open.slots.is_empty(), "the first sweep");
        open.slots.insert(ProjectId::new(), slot(None, 0));
        files.sweep(&mut open, first + millis(SWEEP_EVERY) - 1).await;
        assert_eq!(open.slots.len(), 1, "not again within SWEEP_EVERY");
        files.sweep(&mut open, first + millis(SWEEP_EVERY)).await;
        assert!(open.slots.is_empty(), "again after SWEEP_EVERY");
    }

    #[tokio::test]
    async fn a_swept_file_closes_behind_a_gate() {
        let dir = tempfile::tempdir().unwrap();
        let files = files();
        let project = ProjectId::new();
        let swept = file(dir.path(), "swept").await;
        {
            let mut open = files.open.lock().await;
            open.slots.insert(project, slot(Some(Arc::clone(&swept)), 0));
        }
        // Only the slot may hold the file for the sweep to take it.
        let pool = swept.db.read().clone();
        drop(swept);
        {
            let mut open = files.open.lock().await;
            files.sweep(&mut open, millis(FILE_IDLE)).await;
            assert!(open.slots.is_empty(), "the slot is gone");
            assert!(open.closing.contains_key(&project), "the file is closing");
            drop(open);
        }
        files.flush_all().await.unwrap();
        assert!(pool.is_closed(), "flush_all waited for the close");
    }

    #[tokio::test]
    async fn stop_all_closes_every_file_and_refuses_more() {
        let dir = tempfile::tempdir().unwrap();
        let files = files();
        let open_file = file(dir.path(), "open").await;
        let pool = open_file.db.read().clone();
        files.open.lock().await.slots.insert(ProjectId::new(), slot(Some(open_file), 0));
        files.stop_all().await;
        assert!(pool.is_closed(), "the open file is closed");
        assert!(files.open.lock().await.stopped, "later gets fail");
    }
}
