//! The mailing module: lists with double opt-in and one-click unsubscribe,
//! broadcasts and email sequences. Plan: `.hive/plans/2026-11-23-mailing.md`.

pub mod api;
mod capture;
pub mod contacts;
mod ops;
mod public;
mod render;
pub mod send;
mod store;

use bizkit_core::contacts::ContactCaptured;
use bizkit_core::{Module, ModuleContext};
use utoipa::OpenApi as _;

pub use crate::ops::TESTS_PER_HOUR;

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
        ops::register(cx)?;
        cx.job::<send::Send, _>(send::SendHandler)?;
        cx.job::<send::FanOut, _>(send::FanOutHandler)?;
        cx.subscribe::<ContactCaptured, _>(capture::CaptureSubscriber)?;
        cx.public_routes(public::router(), public::PublicDocs::openapi())
    }
}
