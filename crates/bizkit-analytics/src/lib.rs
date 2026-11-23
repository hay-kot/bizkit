//! The analytics module: web analytics and product events from one snippet
//! and one events store. Web reports and product reports are views over it.

pub mod api;
pub mod names;
mod ops;
mod store;

use bizkit_core::{Module, ModuleContext};

pub use crate::ops::MAX_HOSTNAMES;

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
        ops::register(cx)
    }
}
