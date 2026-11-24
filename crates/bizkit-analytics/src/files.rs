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
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::error::ApiError;
use bizkit_core::ids::ProjectId;
use sqlx::pool::PoolOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};
use tokio::sync::{Mutex, Semaphore};

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

/// Every open project file.
#[derive(Debug)]
pub(crate) struct Files {
    open: Mutex<HashMap<ProjectId, Arc<ProjectFile>>>,
    sites: Arc<Sites>,
    flush: Duration,
    limits: Limits,
    global: Arc<Semaphore>,
}

impl Files {
    pub(crate) fn new(sites: Arc<Sites>, flush: Duration, limits: Limits) -> Self {
        Self {
            open: Mutex::new(HashMap::new()),
            sites,
            flush,
            limits,
            global: Arc::new(Semaphore::new(limits.global_queue)),
        }
    }

    /// The project's file, opening (and creating) it when the project has
    /// a site. `None` when it has none, so a deleted project's cached site
    /// can't bring its file back.
    pub(crate) async fn get(
        &self,
        platform: &Platform,
        scope: Scope,
    ) -> anyhow::Result<Option<Arc<ProjectFile>>> {
        let mut files = self.open.lock().await;
        if let Some(file) = files.get(&scope.project_id) {
            return Ok(Some(Arc::clone(file)));
        }
        let site = store::find_site(platform.db().read(), scope.workspace_id, scope.project_id)
            .await
            .context("checking for a site")?;
        if site.is_none() {
            return Ok(None);
        }
        let data_dir = platform.config().data_dir.as_deref().context("data_dir is not set")?;
        let db = ProjectDb::open(&project_db_path(data_dir, scope.project_id)).await?;
        let file =
            Arc::new(ProjectFile { db: Arc::new(db), writer: Mutex::new(WriterState::Idle) });
        files.insert(scope.project_id, Arc::clone(&file));
        drop(files);
        Ok(Some(file))
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
        let files: Vec<Arc<ProjectFile>> = self.open.lock().await.values().cloned().collect();
        let mut first_err = None;
        for file in files {
            let flushed = match &*file.writer.lock().await {
                WriterState::Running(handle) => Some(handle.flush()),
                WriterState::Idle | WriterState::Stopped => None,
            };
            if let Some(flushed) = flushed
                && let Err(err) = flushed.await
            {
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
        let file = self.open.lock().await.remove(&project_id);
        if let Some(file) = file {
            file.stop().await;
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
        let files: Vec<Arc<ProjectFile>> =
            self.open.lock().await.drain().map(|(_, file)| file).collect();
        for file in files {
            file.stop().await;
        }
    }
}
