//! SQLite access: a read pool and a single-connection write pool.

use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::str::FromStr as _;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use sqlx::migrate::Migrator;
use sqlx::pool::PoolOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};
use tokio::sync::watch;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The database. Reads go to [`Db::read`]; writes go through [`Db::write`].
#[derive(Debug, Clone)]
pub struct Db {
    read: SqlitePool,
    write: SqlitePool,
    wake: Arc<watch::Sender<u64>>,
}

impl Db {
    /// Opens (creating if needed) the database file at `path` in WAL mode.
    ///
    /// # Errors
    ///
    /// When the file can't be opened or created.
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite:")
            .context("building connect options")?
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(BUSY_TIMEOUT);
        let readers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
        let readers = u32::try_from(readers).unwrap_or(4);
        // The write pool opens first so it creates the file and switches it to
        // WAL before readers connect.
        let write = PoolOptions::<Sqlite>::new()
            .max_connections(1)
            // Holding a `WriteTx` while asking for another one would deadlock;
            // the short timeout makes that mistake fail fast.
            .acquire_timeout(BUSY_TIMEOUT)
            .connect_with(options.clone())
            .await
            .with_context(|| format!("opening {}", path.display()))?;
        let read = PoolOptions::<Sqlite>::new()
            .max_connections(readers)
            .connect_with(options.read_only(true))
            .await
            .with_context(|| format!("opening {} for reading", path.display()))?;
        Ok(Self { read, write, wake: Arc::new(watch::Sender::new(0)) })
    }

    /// Wraps one pool for both reads and writes, as `#[sqlx::test]` provides.
    #[must_use]
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { read: pool.clone(), write: pool, wake: Arc::new(watch::Sender::new(0)) }
    }

    /// The read pool. Inside a [`WriteTx`], read through the transaction
    /// instead so you see its own writes.
    #[must_use]
    pub const fn read(&self) -> &SqlitePool {
        &self.read
    }

    /// Starts a write transaction with `BEGIN IMMEDIATE`, which takes the
    /// write lock up front. A deferred transaction that reads first and then
    /// writes after another process committed fails with `SQLITE_BUSY` at
    /// once, and `busy_timeout` doesn't help it.
    ///
    /// Never call this while holding a `WriteTx`: there is one write
    /// connection, so the second call waits out the acquire timeout.
    ///
    /// # Errors
    ///
    /// When no write connection frees up within the 5 s acquire timeout, or `BEGIN IMMEDIATE` fails.
    pub async fn write(&self) -> anyhow::Result<WriteTx> {
        let tx = self
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .context("starting a write transaction")?;
        Ok(WriteTx { tx, jobs_written: false, request_id: None, wake: Arc::clone(&self.wake) })
    }

    /// Runs each migrator in order. Each one ignores versions it doesn't know,
    /// because core and every module share `_sqlx_migrations`.
    ///
    /// # Errors
    ///
    /// When a migration fails or a recorded migration's checksum changed.
    pub async fn migrate(&self, migrators: Vec<Migrator>) -> anyhow::Result<()> {
        for mut migrator in migrators {
            migrator.set_ignore_missing(true);
            migrator.run(&self.write).await.context("running migrations")?;
        }
        Ok(())
    }

    /// Watches commits that wrote jobs. Each one bumps the value, and a
    /// receiver's `changed()` sees every bump since it last looked, so a
    /// commit that lands while a worker loop is busy still wakes it.
    pub(crate) fn jobs_written(&self) -> watch::Receiver<u64> {
        self.wake.subscribe()
    }

    /// Closes both pools.
    pub async fn close(&self) {
        self.write.close().await;
        self.read.close().await;
    }
}

/// A write transaction. Dereferences to the connection, so store functions
/// take `&mut *tx`.
#[derive(Debug)]
pub struct WriteTx {
    tx: Transaction<'static, Sqlite>,
    jobs_written: bool,
    request_id: Option<String>,
    wake: Arc<watch::Sender<u64>>,
}

impl WriteTx {
    /// Tags jobs written in this transaction with the request that caused them.
    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// The request id jobs written here carry.
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub(crate) const fn mark_jobs_written(&mut self) {
        self.jobs_written = true;
    }

    /// Commits, then wakes the worker if this transaction wrote jobs.
    ///
    /// # Errors
    ///
    /// When the commit fails; the transaction is rolled back.
    pub async fn commit(self) -> anyhow::Result<()> {
        self.tx.commit().await.context("committing")?;
        if self.jobs_written {
            self.wake.send_modify(|count| *count = count.wrapping_add(1));
        }
        Ok(())
    }
}

impl Deref for WriteTx {
    type Target = SqliteConnection;

    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}

impl DerefMut for WriteTx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tx
    }
}
