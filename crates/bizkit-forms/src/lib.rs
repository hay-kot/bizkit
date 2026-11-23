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

#[cfg(feature = "test-hooks")]
use bizkit_core::DemoSeed;
use bizkit_core::{Module, ModuleContext};
#[cfg(feature = "test-hooks")]
use futures::future::BoxFuture;
use utoipa::OpenApi as _;

use crate::hooks::{FormsHookSource, PRE_VALIDATE, SUBMISSION_CREATED, SubmissionCreated};

/// The forms module.
#[derive(Debug, Clone, Copy, Default)]
pub struct Forms;

impl Module for Forms {
    fn name(&self) -> &'static str {
        "forms"
    }

    fn migrations(&self) -> Option<sqlx::migrate::Migrator> {
        Some(sqlx::migrate!())
    }

    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
        ops::register(cx)?;
        cx.sync_hook(PRE_VALIDATE)?;
        cx.async_hook::<SubmissionCreated, _>(SUBMISSION_CREATED, FormsHookSource)?;
        cx.public_routes(ingest::route::router(), ingest::route::IngestDocs::openapi())
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
