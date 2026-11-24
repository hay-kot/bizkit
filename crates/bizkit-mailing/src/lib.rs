//! The mailing module: lists with double opt-in and one-click unsubscribe,
//! broadcasts and email sequences. Plan: `.hive/plans/2026-11-23-mailing.md`.

pub mod api;

use bizkit_core::{Module, ModuleContext};

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
        cx.scope_area("mailing", "Mailing lists, contacts, broadcasts and sequences")
    }
}
