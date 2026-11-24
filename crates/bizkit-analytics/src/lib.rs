//! The analytics module: web analytics and product events from one snippet
//! and one events store. Web reports and product reports are views over it.

pub mod api;
pub mod beacon;
mod cache;
pub mod enrich;
mod explore;
mod files;
mod funnels;
pub mod geo;
pub mod hooks;
mod ingest;
pub mod limit;
pub mod names;
mod ops;
mod people;
mod report;
mod rollup;
#[cfg(feature = "test-hooks")]
mod seed;
mod server;
mod stats;
mod store;
pub mod testing;
pub mod ua;
mod write;
mod writer;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, bail};
#[cfg(feature = "test-hooks")]
use bizkit_core::DemoSeed;
use bizkit_core::api::ProjectDeleted;
use bizkit_core::jobs::{JobCtx, Subscriber};
use bizkit_core::{Lifecycle, Module, ModuleContext};
use futures::future::BoxFuture;
use serde::Deserialize;
use utoipa::OpenApi as _;

use crate::files::{Files, Limits};
pub use crate::files::{project_db_path, project_migrator};
pub use crate::ops::MAX_HOSTNAMES;

/// The default flush window, in milliseconds.
const DEFAULT_FLUSH_MS: u16 = 100;
/// The flush window's bounds. Above 250 ms a crash loses more than Hayden
/// accepted; below 10 ms the writer commits nearly per request.
const FLUSH_MS: std::ops::RangeInclusive<u16> = 10..=250;

/// The `modules.analytics` config section.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsConfig {
    /// A MaxMind-format country database (such as DB-IP's free country
    /// file). Without it, countries are unknown.
    #[serde(default)]
    pub geoip_path: Option<PathBuf>,
    /// Days of raw events to keep; default 90, at least 7.
    #[serde(default)]
    pub raw_retention_days: Option<u16>,
    /// Milliseconds events wait in memory before they are written; default
    /// 100, 10 to 250. A crash loses at most this window of events.
    #[serde(default)]
    pub flush_ms: Option<u16>,
}

/// The analytics module.
#[derive(Debug, Clone, Copy, Default)]
pub struct Analytics {
    limits: Limits,
}

/// Constructors for tests.
impl Analytics {
    /// No timer flushes: queued events are written only by
    /// `Platform::flush_modules` (`TestApp::flush`) or once 1,000 wait, so
    /// tests never race the timer.
    #[must_use]
    pub fn manual_flush() -> Self {
        Self { limits: Limits { timer: false, ..Limits::default() } }
    }

    /// [`Analytics::manual_flush`] with room for only `events` queued
    /// events per project, to reach the full-queue path.
    #[must_use]
    pub fn manual_flush_with_queue(events: usize) -> Self {
        Self { limits: Limits { timer: false, project_queue: events, ..Limits::default() } }
    }
}

/// Flushes the project files' writers on demand and on shutdown.
#[derive(Debug)]
struct AnalyticsLifecycle(Arc<Files>);

impl Lifecycle for AnalyticsLifecycle {
    fn flush(&self) -> BoxFuture<'_, anyhow::Result<()>> {
        Box::pin(self.0.flush_all())
    }

    fn stop(&self) -> BoxFuture<'_, ()> {
        Box::pin(self.0.stop_all())
    }
}

/// Deletes a deleted project's analytics file. Its rows in the app
/// database go by foreign key.
struct DeleteProjectFile(Arc<Files>);

impl Subscriber<ProjectDeleted> for DeleteProjectFile {
    const NAME: &'static str = "analytics";

    async fn handle(&self, cx: JobCtx, event: ProjectDeleted) -> anyhow::Result<()> {
        let data_dir = cx.platform.config().data_dir.as_deref().context("data_dir is not set")?;
        self.0.remove(data_dir, event.project_id).await
    }
}

impl Module for Analytics {
    fn name(&self) -> &'static str {
        "analytics"
    }

    fn migrations(&self) -> Option<sqlx::migrate::Migrator> {
        Some(sqlx::migrate!())
    }

    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
        let config: AnalyticsConfig = cx.config()?;
        let flush_ms = config.flush_ms.unwrap_or(DEFAULT_FLUSH_MS);
        if !FLUSH_MS.contains(&flush_ms) {
            bail!(
                "modules.analytics.flush_ms is {flush_ms}; it must be {} to {}",
                FLUSH_MS.start(),
                FLUSH_MS.end()
            );
        }
        let sites = Arc::new(cache::Sites::default());
        let flush = Duration::from_millis(u64::from(flush_ms));
        let files = Arc::new(Files::new(Arc::clone(&sites), flush, self.limits));
        let geo = geo::Geo::new(config.geoip_path);
        let ingest = Arc::new(ingest::Ingest::new(geo, sites, Arc::clone(&files)));
        let retention_days = config
            .raw_retention_days
            .unwrap_or(rollup::DEFAULT_RETENTION_DAYS)
            .max(rollup::MIN_RETENTION_DAYS);
        let rollup = rollup::RollupHandler { retention_days, files: Arc::clone(&files) };
        cx.job::<rollup::Rollup, _>(rollup)?;
        ops::register(cx, &ingest)?;
        stats::register(cx, &files)?;
        explore::register(cx, &files)?;
        funnels::register(cx, &files)?;
        people::register(cx, &files)?;
        cx.hook_target_kind(&hooks::GOAL, hooks::GoalTargets)?;
        let source = hooks::GoalHookSource(Arc::clone(&files));
        cx.async_hook::<hooks::GoalConverted, _>(hooks::GOAL_CONVERTED, source)?;
        cx.subscribe::<ProjectDeleted, _>(DeleteProjectFile(Arc::clone(&files)))?;
        cx.lifecycle(AnalyticsLifecycle(files))?;
        cx.public_routes(ingest::router(ingest), ingest::IngestDocs::openapi())
    }

    #[cfg(feature = "test-hooks")]
    fn seed_demo<'a>(
        &'a self,
        db: &'a bizkit_core::db::Db,
        seed: &'a mut DemoSeed,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(seed::seed(db, seed))
    }
}
