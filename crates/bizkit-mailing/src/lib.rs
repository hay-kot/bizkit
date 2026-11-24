//! The mailing module: lists with double opt-in and one-click unsubscribe,
//! broadcasts and email sequences. Plan: `.hive/plans/2026-11-23-mailing.md`.

pub mod api;
mod capture;
pub mod contacts;
mod events;
pub mod hooks;
mod ops;
mod public;
mod render;
#[cfg(feature = "test-hooks")]
mod seed;
pub mod send;
mod sequence_ops;
mod sequences;
mod store;
mod suppress;
mod suppression_ops;

#[cfg(feature = "test-hooks")]
use bizkit_core::DemoSeed;
use bizkit_core::contacts::ContactCaptured;
use bizkit_core::{Module, ModuleContext};
#[cfg(feature = "test-hooks")]
use futures::future::BoxFuture;
use utoipa::OpenApi as _;

pub use crate::ops::TESTS_PER_HOUR;
pub use crate::sequence_ops::{MAX_DELAY_HOURS, MAX_STEPS};

/// The mailing module.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mailing;

impl Module for Mailing {
    fn name(&self) -> &'static str {
        "mailing"
    }

    fn migrations(&self) -> Option<sqlx::migrate::Migrator> {
        Some(sqlx::migrate!())
    }

    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
        cx.hook_target_kind(&hooks::SEQUENCE, hooks::SequenceTargets)?;
        cx.sync_hook(hooks::STEP_CONDITION)?;
        ops::register(cx)?;
        cx.job::<send::Send, _>(send::SendHandler)?;
        cx.job::<sequences::Advance, _>(sequences::AdvanceHandler)?;
        cx.job::<send::FanOut, _>(send::FanOutHandler)?;
        cx.subscribe::<ContactCaptured, _>(capture::CaptureSubscriber)?;
        cx.public_routes(public::router(), public::PublicDocs::openapi())
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
