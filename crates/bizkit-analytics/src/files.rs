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
use std::sync::atomic::{AtomicU64, Ordering};
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
}

/// Why `get` has no file for a project.
enum Unopened {
    NoSite,
    Failed(anyhow::Error),
}

/// Every open project file. A file nobody has asked for in [`FILE_IDLE`]
/// is closed by the next sweep and opens again on its next use.
#[derive(Debug)]
pub(crate) struct Files {
    open: Mutex<HashMap<ProjectId, Arc<Slot>>>,
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
            open: Mutex::new(HashMap::new()),
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
    /// cell, so a slow open holds up only that project.
    pub(crate) async fn get(
        &self,
        platform: &Platform,
        scope: Scope,
    ) -> anyhow::Result<Option<Arc<ProjectFile>>> {
        let now = self.clock();
        let (slot, idle) = {
            let mut open = self.open.lock().await;
            let slot = Arc::clone(open.entry(scope.project_id).or_default());
            slot.used.store(now, Ordering::Relaxed);
            let idle = self.sweep(&mut open, now);
            drop(open);
            (slot, idle)
        };
        for file in idle {
            file.stop().await;
        }
        let opened = slot.file.get_or_try_init(|| open_file(platform, scope)).await.cloned();
        match opened {
            Ok(file) => Ok(Some(file)),
            Err(unopened) => {
                self.forget(scope.project_id, &slot).await;
                match unopened {
                    Unopened::NoSite => Ok(None),
                    Unopened::Failed(err) => Err(err),
                }
            }
        }
    }

    /// Takes the idle files out of `open`, at most once per [`SWEEP_EVERY`].
    fn sweep(&self, open: &mut HashMap<ProjectId, Arc<Slot>>, now: u64) -> Vec<Arc<ProjectFile>> {
        let last = self.swept.load(Ordering::Relaxed);
        if now.saturating_sub(last) < millis(SWEEP_EVERY) {
            return Vec::new();
        }
        self.swept.store(now, Ordering::Relaxed);
        take_idle(open, now)
    }

    /// Drops a slot whose open failed or found no site, unless another
    /// caller is still waiting on it and may open it yet.
    async fn forget(&self, project_id: ProjectId, slot: &Arc<Slot>) {
        let mut open = self.open.lock().await;
        let ours = open.get(&project_id).is_some_and(|held| Arc::ptr_eq(held, slot));
        // The map and this caller: nobody else can still fill the cell.
        if ours && slot.file.get().is_none() && Arc::strong_count(slot) == 2 {
            open.remove(&project_id);
        }
    }

    /// Every file open now.
    async fn files(&self) -> Vec<Arc<ProjectFile>> {
        self.open.lock().await.values().filter_map(|slot| slot.file.get().cloned()).collect()
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

    /// Writes everything queued in every file, side effects included.
    pub(crate) async fn flush_all(&self) -> anyhow::Result<()> {
        let mut first_err = None;
        for file in self.files().await {
            if let Err(err) = file.flush().await {
                first_err.get_or_insert(err);
            }
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
        let slot = self.open.lock().await.remove(&project_id);
        if let Some(slot) = slot {
            // Waits for an open in progress and opens nothing itself: the
            // init fails at once when no open is running.
            let opened =
                slot.file.get_or_try_init(|| async { Err::<_, Unopened>(Unopened::NoSite) }).await;
            if let Ok(file) = opened {
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

    /// Flushes and stops every writer and closes every file.
    pub(crate) async fn stop_all(&self) {
        let slots: Vec<Arc<Slot>> = self.open.lock().await.drain().map(|(_, slot)| slot).collect();
        for slot in slots {
            if let Some(file) = slot.file.get() {
                file.stop().await;
            }
        }
    }
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
fn take_idle(open: &mut HashMap<ProjectId, Arc<Slot>>, now: u64) -> Vec<Arc<ProjectFile>> {
    let mut idle = Vec::new();
    open.retain(|_, slot| {
        let unused = now.saturating_sub(slot.used.load(Ordering::Relaxed)) >= millis(FILE_IDLE);
        if !unused || Arc::strong_count(slot) > 1 {
            return true;
        }
        match slot.file.get() {
            Some(file) if Arc::strong_count(file) > 1 => true,
            Some(file) => {
                idle.push(Arc::clone(file));
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
        Arc::new(Slot { file: OnceCell::new_with(file), used: AtomicU64::new(used) })
    }

    #[tokio::test]
    async fn only_idle_unshared_files_are_taken() {
        let dir = tempfile::tempdir().unwrap();
        let now = millis(FILE_IDLE) + 1_000;
        let [idle, recent, held, waited, empty] = std::array::from_fn(|_| ProjectId::new());
        let held_file = file(dir.path(), "held").await;
        let waited_slot = slot(Some(file(dir.path(), "waited").await), 0);
        let mut open = HashMap::from([
            (idle, slot(Some(file(dir.path(), "idle").await), 1_000)),
            (recent, slot(Some(file(dir.path(), "recent").await), 1_001)),
            (held, slot(Some(Arc::clone(&held_file)), 0)),
            (waited, Arc::clone(&waited_slot)),
            (empty, slot(None, 0)),
        ]);

        let taken = take_idle(&mut open, now);

        assert_eq!(taken.len(), 1, "only the idle file");
        assert!(!open.contains_key(&idle), "the idle file's slot is gone");
        assert!(open.contains_key(&recent), "used within FILE_IDLE");
        assert!(open.contains_key(&held), "an operation still holds the file");
        assert!(open.contains_key(&waited), "a get still holds the slot");
        assert!(!open.contains_key(&empty), "a slot that never opened is dropped");
        for file in taken.into_iter().chain([held_file]) {
            file.stop().await;
        }
        for slot in open.into_values() {
            if let Some(file) = slot.file.get() {
                file.stop().await;
            }
        }
    }

    #[test]
    fn sweeps_at_most_once_per_interval() {
        let files = Files::new(Arc::default(), Duration::from_millis(100), Limits::default());
        let mut open = HashMap::from([(ProjectId::new(), slot(None, 0))]);
        // Slots last used at 0 are idle from FILE_IDLE on.
        let first = millis(FILE_IDLE);
        files.sweep(&mut open, first);
        assert!(open.is_empty(), "the first sweep");
        open.insert(ProjectId::new(), slot(None, 0));
        files.sweep(&mut open, first + millis(SWEEP_EVERY) - 1);
        assert_eq!(open.len(), 1, "not again within SWEEP_EVERY");
        files.sweep(&mut open, first + millis(SWEEP_EVERY));
        assert!(open.is_empty(), "again after SWEEP_EVERY");
    }
}
