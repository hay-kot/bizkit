//! The forms module: form definitions per project, a public ingest endpoint
//! for static sites with bot checks, and submissions with CSV export.

pub mod api;
pub mod export;
pub mod fields;
pub mod hooks;
pub mod ingest;
pub mod notify;
mod ops;
#[cfg(feature = "test-hooks")]
mod seed;
mod store;

use std::sync::Arc;

#[cfg(feature = "test-hooks")]
use bizkit_core::DemoSeed;
use bizkit_core::{Module, ModuleContext};
#[cfg(feature = "test-hooks")]
use futures::future::BoxFuture;
use utoipa::OpenApi as _;

use crate::hooks::{NoopHook, SubmitHook};

/// The forms module.
#[derive(Debug, Clone)]
pub struct Forms {
    hook: Arc<dyn SubmitHook>,
}

impl Default for Forms {
    fn default() -> Self {
        Self { hook: Arc::new(NoopHook) }
    }
}

impl Forms {
    /// The module with `hook` run before field validation.
    #[must_use]
    pub fn with_hook(hook: impl SubmitHook) -> Self {
        Self { hook: Arc::new(hook) }
    }
}

impl Module for Forms {
    fn name(&self) -> &'static str {
        "forms"
    }

    fn migrations(&self) -> Option<sqlx::migrate::Migrator> {
        Some(sqlx::migrate!())
    }

    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
        ops::register(cx)?;
        cx.public_routes(
            ingest::route::router(Arc::clone(&self.hook)),
            ingest::route::IngestDocs::openapi(),
        )
    }

    #[cfg(feature = "test-hooks")]
    fn seed_demo<'a>(
        &'a self,
        db: &'a bizkit_core::db::Db,
        seed: &'a DemoSeed,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(seed::seed(db, seed))
    }
}
