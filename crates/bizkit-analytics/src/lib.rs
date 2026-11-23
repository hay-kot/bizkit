//! The analytics module: web analytics and product events from one snippet
//! and one events store. Web reports and product reports are views over it.

pub mod api;
pub mod beacon;
mod cache;
pub mod enrich;
pub mod geo;
pub mod hooks;
mod ingest;
pub mod limit;
pub mod names;
mod ops;
mod server;
mod store;
pub mod ua;
mod write;

use std::path::PathBuf;
use std::sync::Arc;

use bizkit_core::{Module, ModuleContext};
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
        ops::register(cx, &ingest)?;
        cx.public_routes(ingest::router(ingest), ingest::IngestDocs::openapi())
    }
}
