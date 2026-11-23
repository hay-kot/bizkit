//! The bench's SQLite files: one per project or one shared with the app
//! stand-in, each with a single write connection like `bizkit_core::db`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr as _;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use sqlx::pool::PoolOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

const SCHEMA: &str = include_str!("schema.sql");
const APP_SCHEMA: &str = include_str!("app.sql");
const APP_FILE: &str = "app.db";
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
/// SQLite allows 32766 bound parameters; a row binds 21.
const ROWS_PER_INSERT: usize = 1500;
/// Dimensions the rollup groups by, as in `analytics::rollup`.
const DIMENSIONS: [&str; 9] =
    ["total", "page", "referrer", "utm_source", "country", "device", "browser", "os", "event"];

/// Where events go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    /// One file per project.
    PerProject,
    /// Every project in the app's file, as production does today.
    Shared,
}

/// The `synchronous` pragma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Sync {
    /// WAL with `synchronous=NORMAL`, as `bizkit_core::db` opens files.
    Normal,
    /// `synchronous=FULL`: an fsync on every commit.
    Full,
}

/// One `analytics_events` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The bench's project number.
    pub project: u32,
    /// Receive time, Unix milliseconds.
    pub ts: i64,
    /// UTC day of `ts`.
    pub day: String,
    /// Event name.
    pub name: String,
    /// Page path.
    pub path: Option<String>,
    /// Page host.
    pub host: Option<String>,
    /// Referrer, UTM and country.
    pub attribution: [Option<String>; 5],
    /// Device, browser and OS.
    pub agent: [String; 3],
    /// Daily visitor hash.
    pub visitor: i64,
    /// Person key.
    pub person: String,
    /// Props JSON, which carries the load generator's `seq`.
    pub props: Option<String>,
}

/// One SQLite file: a single write connection and a small read pool.
#[derive(Debug)]
pub struct DbFile {
    path: PathBuf,
    write: SqlitePool,
    read: SqlitePool,
}

impl DbFile {
    async fn open(path: &Path, sync: Sync, schema: &[&'static str]) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite:")
            .context("building connect options")?
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(match sync {
                Sync::Normal => SqliteSynchronous::Normal,
                Sync::Full => SqliteSynchronous::Full,
            })
            .busy_timeout(BUSY_TIMEOUT);
        let write = PoolOptions::<Sqlite>::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(30))
            .connect_with(options.clone())
            .await
            .with_context(|| format!("opening {}", path.display()))?;
        for part in schema {
            sqlx::raw_sql(*part).execute(&write).await.context("creating the schema")?;
        }
        let read = PoolOptions::<Sqlite>::new()
            .max_connections(2)
            .connect_with(options.read_only(true))
            .await
            .with_context(|| format!("opening {} for reading", path.display()))?;
        Ok(Self { path: path.to_owned(), write, read })
    }

    /// Bytes in the database file and its WAL.
    #[must_use]
    pub fn sizes(&self) -> (u64, u64) {
        let size = |path: &Path| std::fs::metadata(path).map_or(0, |meta| meta.len());
        let mut wal = self.path.clone().into_os_string();
        wal.push("-wal");
        (size(&self.path), size(Path::new(&wal)))
    }

    async fn close(&self) {
        self.write.close().await;
        self.read.close().await;
    }
}

/// Every file the bench writes.
#[derive(Debug)]
pub struct Store {
    data: PathBuf,
    layout: Layout,
    sync: Sync,
    app: Arc<DbFile>,
    projects: tokio::sync::Mutex<HashMap<u32, Arc<DbFile>>>,
    ids: Mutex<HashMap<u32, (String, String)>>,
}

impl Store {
    /// Opens the app file; project files open on first use.
    ///
    /// # Errors
    ///
    /// When the app file can't be opened.
    pub async fn open(data: &Path, layout: Layout, sync: Sync) -> anyhow::Result<Self> {
        std::fs::create_dir_all(data).with_context(|| format!("creating {}", data.display()))?;
        let schema: &[&'static str] = match layout {
            Layout::PerProject => &[APP_SCHEMA],
            Layout::Shared => &[APP_SCHEMA, SCHEMA],
        };
        let app = DbFile::open(&data.join(APP_FILE), sync, schema).await?;
        Ok(Self {
            data: data.to_owned(),
            layout,
            sync,
            app: Arc::new(app),
            projects: tokio::sync::Mutex::new(HashMap::new()),
            ids: Mutex::new(HashMap::new()),
        })
    }

    /// Which writer a project's events go to: the project in `per-project`,
    /// 0 for the one shared file.
    #[must_use]
    pub const fn writer_key(&self, project: u32) -> u32 {
        match self.layout {
            Layout::PerProject => project,
            Layout::Shared => 0,
        }
    }

    /// The file a project's events live in.
    ///
    /// # Errors
    ///
    /// When a project file can't be opened.
    pub async fn file(&self, project: u32) -> anyhow::Result<Arc<DbFile>> {
        if self.layout == Layout::Shared {
            return Ok(Arc::clone(&self.app));
        }
        let mut files = self.projects.lock().await;
        if let Some(file) = files.get(&project) {
            return Ok(Arc::clone(file));
        }
        let path = self.data.join(format!("p{project}.db"));
        let file = Arc::new(DbFile::open(&path, self.sync, &[SCHEMA]).await?);
        files.insert(project, Arc::clone(&file));
        drop(files);
        Ok(file)
    }

    /// The workspace and project IDs stored for a bench project number.
    fn ids(&self, project: u32) -> (String, String) {
        let mut ids = self.ids.lock().unwrap_or_else(PoisonError::into_inner);
        ids.entry(project).or_insert_with(|| (format!("w{project}"), format!("p{project}"))).clone()
    }

    /// Inserts rows of one writer in one `BEGIN IMMEDIATE` transaction.
    ///
    /// # Errors
    ///
    /// When the transaction fails.
    pub async fn insert_batch(&self, key: u32, rows: &[Row]) -> anyhow::Result<()> {
        let file = self.file(key).await?;
        let mut tx = file.write.begin_with("BEGIN IMMEDIATE").await.context("begin")?;
        for chunk in rows.chunks(ROWS_PER_INSERT) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "INSERT INTO analytics_events (workspace_id, project_id, ts, day, name, path, host,
                   referrer_host, utm_source, utm_medium, utm_campaign, country, device, browser,
                   os, visitor, person, anon_id, user_id, props, source) ",
            );
            query.push_values(chunk, |mut values, row| {
                let (workspace, project) = self.ids(row.project);
                let [referrer, source, medium, campaign, country] = row.attribution.clone();
                let [device, browser, os] = row.agent.clone();
                values
                    .push_bind(workspace)
                    .push_bind(project)
                    .push_bind(row.ts)
                    .push_bind(row.day.clone())
                    .push_bind(row.name.clone())
                    .push_bind(row.path.clone())
                    .push_bind(row.host.clone())
                    .push_bind(referrer)
                    .push_bind(source)
                    .push_bind(medium)
                    .push_bind(campaign)
                    .push_bind(country)
                    .push_bind(device)
                    .push_bind(browser)
                    .push_bind(os)
                    .push_bind(row.visitor)
                    .push_bind(row.person.clone())
                    .push_bind(None::<String>)
                    .push_bind(None::<String>)
                    .push_bind(row.props.clone())
                    .push_bind("web");
            });
            query.build().execute(&mut *tx).await.context("inserting events")?;
        }
        tx.commit().await.context("commit")?;
        Ok(())
    }

    /// A sign-in-shaped write on the app file: read the user, add a session.
    ///
    /// # Errors
    ///
    /// When the transaction fails.
    pub async fn sign_in(&self, now: i64) -> anyhow::Result<()> {
        let mut tx = self.app.write.begin_with("BEGIN IMMEDIATE").await.context("begin")?;
        let user: i64 = sqlx::query_scalar("SELECT id FROM users WHERE email = 'demo@bizkit.test'")
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO sessions (user_id, created_at) VALUES (?, ?)")
            .bind(user)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The overview's queries over one day of raw events: totals, then the
    /// top pages. Returns how many rows came back.
    ///
    /// # Errors
    ///
    /// When a query fails.
    pub async fn dashboard(&self, project: u32, day: &str) -> anyhow::Result<usize> {
        let file = self.file(project).await?;
        let (workspace, project_id) = self.ids(project);
        let totals = sqlx::query(
            "SELECT COUNT(DISTINCT visitor), SUM(name = 'pageview'), SUM(name <> 'pageview')
             FROM analytics_events WHERE workspace_id = ? AND project_id = ? AND day = ?",
        )
        .bind(&workspace)
        .bind(&project_id)
        .bind(day)
        .fetch_all(&file.read)
        .await?;
        let pages = sqlx::query(
            "SELECT path, COUNT(DISTINCT visitor) AS visitors, COUNT(*) FROM analytics_events
             WHERE workspace_id = ? AND project_id = ? AND day = ? AND name = 'pageview'
             GROUP BY path ORDER BY visitors DESC LIMIT 10",
        )
        .bind(&workspace)
        .bind(&project_id)
        .bind(day)
        .fetch_all(&file.read)
        .await?;
        Ok(totals.len() + pages.len())
    }

    /// The rollup of one day for one project, as `analytics::rollup` does it.
    /// Returns how long it held the write connection.
    ///
    /// # Errors
    ///
    /// When the transaction fails.
    pub async fn rollup(&self, project: u32, day: &str) -> anyhow::Result<Duration> {
        let file = self.file(project).await?;
        let (workspace, project_id) = self.ids(project);
        let started = Instant::now();
        let mut tx = file.write.begin_with("BEGIN IMMEDIATE").await.context("begin")?;
        sqlx::query("DELETE FROM analytics_daily WHERE project_id = ? AND day = ?")
            .bind(&project_id)
            .bind(day)
            .execute(&mut *tx)
            .await?;
        for dimension in DIMENSIONS {
            sqlx::query(
                "INSERT INTO analytics_daily (project_id, day, dimension, value, visitors, pageviews, events)
                 SELECT ?, ?, ?, value, COUNT(DISTINCT visitor), SUM(name = 'pageview'), SUM(name <> 'pageview')
                 FROM (
                   SELECT COALESCE(CASE ? WHEN 'page' THEN path WHEN 'referrer' THEN referrer_host
                                          WHEN 'utm_source' THEN utm_source WHEN 'country' THEN country
                                          WHEN 'device' THEN device WHEN 'browser' THEN browser
                                          WHEN 'os' THEN os WHEN 'event' THEN name END, '') AS value,
                          visitor, name
                   FROM analytics_events
                   WHERE workspace_id = ? AND project_id = ? AND day = ?
                     AND CASE ? WHEN 'total' THEN 1 WHEN 'event' THEN name <> 'pageview' ELSE name = 'pageview' END)
                 GROUP BY value",
            )
            .bind(&project_id)
            .bind(day)
            .bind(dimension)
            .bind(dimension)
            .bind(&workspace)
            .bind(&project_id)
            .bind(day)
            .bind(dimension)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(started.elapsed())
    }

    /// Summed database and WAL bytes over every file.
    pub async fn sizes(&self) -> (u64, u64) {
        let files = self.projects.lock().await;
        files.values().chain(std::iter::once(&self.app)).fold((0, 0), |(db, wal), file| {
            let (d, w) = file.sizes();
            (db + d, wal + w)
        })
    }

    /// Closes every file.
    pub async fn close(&self) {
        let files: Vec<Arc<DbFile>> = self.projects.lock().await.values().cloned().collect();
        for file in files {
            file.close().await;
        }
        self.app.close().await;
    }
}

/// Reads the `seq` of every stored event under `data`, from every `.db`
/// file. It opens them read-write, as a restarted server would, so SQLite
/// recovers a WAL a crash left behind.
///
/// # Errors
///
/// When a file can't be read.
pub async fn stored_seqs(data: &Path) -> anyhow::Result<Vec<u64>> {
    let mut seqs = Vec::new();
    let entries = std::fs::read_dir(data).with_context(|| format!("reading {}", data.display()))?;
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "db") {
            continue;
        }
        let options = SqliteConnectOptions::from_str("sqlite:")?.filename(&path);
        let pool = PoolOptions::<Sqlite>::new().max_connections(1).connect_with(options).await?;
        let has_events: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'analytics_events')",
        )
        .fetch_one(&pool)
        .await?;
        if has_events {
            let found: Vec<Option<i64>> =
                sqlx::query_scalar("SELECT json_extract(props, '$.seq') FROM analytics_events")
                    .fetch_all(&pool)
                    .await?;
            seqs.extend(found.into_iter().flatten().filter_map(|seq| u64::try_from(seq).ok()));
        }
        pool.close().await;
    }
    Ok(seqs)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn row(project: u32, seq: u64) -> Row {
        Row {
            project,
            ts: 1,
            day: "2026-01-01".to_owned(),
            name: "pageview".to_owned(),
            path: Some("/".to_owned()),
            host: Some("x.test".to_owned()),
            attribution: [None, None, None, None, None],
            agent: ["desktop".to_owned(), "Chrome".to_owned(), "macOS".to_owned()],
            visitor: 7,
            person: "v:2026-01-01:7".to_owned(),
            props: Some(format!(r#"{{"seq":{seq}}}"#)),
        }
    }

    #[tokio::test]
    async fn batches_land_in_their_project_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), Layout::PerProject, Sync::Normal).await.unwrap();
        let rows: Vec<Row> = (0..3000).map(|seq| row(1, seq)).collect();
        store.insert_batch(1, &rows).await.unwrap();
        store.insert_batch(2, &[row(2, 5000)]).await.unwrap();
        assert!(store.dashboard(1, "2026-01-01").await.unwrap() > 0, "queries run");
        store.rollup(1, "2026-01-01").await.unwrap();
        store.sign_in(1).await.unwrap();
        store.close().await;
        let mut seqs = stored_seqs(dir.path()).await.unwrap();
        seqs.sort_unstable();
        assert_eq!(seqs.len(), 3001, "every row stored");
        assert_eq!(seqs.last(), Some(&5000), "project 2 has its own file");
        assert!(dir.path().join("p2.db").exists(), "a file per project");
    }

    #[tokio::test]
    async fn shared_layout_writes_into_the_app_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), Layout::Shared, Sync::Full).await.unwrap();
        assert_eq!(store.writer_key(9), 0, "one writer");
        store.insert_batch(0, &[row(9, 1)]).await.unwrap();
        store.close().await;
        assert!(!dir.path().join("p9.db").exists(), "no project files");
        assert_eq!(stored_seqs(dir.path()).await.unwrap(), [1]);
    }
}
