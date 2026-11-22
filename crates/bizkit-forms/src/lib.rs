//! The forms module: form definitions per project, a public ingest endpoint
//! for static sites with bot checks, and submissions with CSV export.

pub mod api;
pub mod export;
pub mod fields;
pub mod hooks;
pub mod ingest;
pub mod notify;
mod ops;
mod store;

use std::sync::Arc;

use bizkit_core::{Module, ModuleContext};
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
}
