//! The extensions module: workspace admins attach JavaScript to the hook
//! points other modules declare. Scripts run in QuickJS sandboxes on their
//! own threads; see [`sandbox`] for the limits and [`runner`] for how runs
//! are recorded.

pub mod api;
mod ops;
mod redact;
pub mod runner;
pub mod sandbox;
#[cfg(feature = "test-hooks")]
mod seed;
mod store;

#[cfg(feature = "test-hooks")]
use bizkit_core::DemoSeed;
use bizkit_core::{Module, ModuleContext};
#[cfg(feature = "test-hooks")]
use futures::future::BoxFuture;

use crate::runner::{RunScriptHandler, ScriptRunner};

/// The extensions module. It provides the platform's hook runner.
#[derive(Debug, Clone, Default)]
pub struct Extensions {
    runner: ScriptRunner,
}

impl Module for Extensions {
    fn name(&self) -> &'static str {
        "extensions"
    }

    fn migrations(&self) -> Option<sqlx::migrate::Migrator> {
        Some(sqlx::migrate!())
    }

    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
        ops::register(cx, &self.runner)?;
        cx.job(RunScriptHandler { runner: self.runner.clone() })?;
        cx.hook_runner(self.runner.clone())
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
