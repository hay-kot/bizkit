//! The analytics module: web analytics and product events from one snippet
//! and one events store. Web reports and product reports are views over it.

pub mod api;
pub mod beacon;
mod cache;
pub mod enrich;
mod explore;
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
pub mod ua;
mod write;

use std::path::PathBuf;
use std::sync::Arc;

#[cfg(feature = "test-hooks")]
use bizkit_core::DemoSeed;
use bizkit_core::{Module, ModuleContext};
#[cfg(feature = "test-hooks")]
use futures::future::BoxFuture;
use serde::Deserialize;
use utoipa::OpenApi as _;

pub use crate::ops::MAX_HOSTNAMES;

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
}

/// The analytics module.
#[derive(Debug, Clone, Copy, Default)]
pub struct Analytics;

impl Module for Analytics {
    fn name(&self) -> &'static str {
        "analytics"
    }

    fn migrations(&self) -> Option<sqlx::migrate::Migrator> {
        Some(sqlx::migrate!())
    }

    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
        let config: AnalyticsConfig = cx.config()?;
        let ingest = Arc::new(ingest::Ingest::new(geo::Geo::new(config.geoip_path)));
        let retention_days = config
            .raw_retention_days
            .unwrap_or(rollup::DEFAULT_RETENTION_DAYS)
            .max(rollup::MIN_RETENTION_DAYS);
        cx.job::<rollup::Rollup, _>(rollup::RollupHandler { retention_days })?;
        ops::register(cx, &ingest)?;
        stats::register(cx)?;
        explore::register(cx)?;
        funnels::register(cx)?;
        people::register(cx)?;
        cx.hook_target_kind(&hooks::GOAL, hooks::GoalTargets)?;
        cx.async_hook::<hooks::GoalConverted, _>(hooks::GOAL_CONVERTED, hooks::GoalHookSource)?;
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
