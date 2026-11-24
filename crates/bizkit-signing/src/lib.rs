//! The doc signing module: a PDF goes out for signature, each signer signs
//! on a public page through a private link, and bizkit seals the final PDF
//! with the signatures and an audit trail.

pub mod api;
mod mail;
mod ops;
mod pdf;
mod store;
pub mod testing;

use bizkit_core::{Module, ModuleContext};

/// The signing module.
#[derive(Debug, Clone, Copy, Default)]
pub struct Signing;

impl Module for Signing {
    fn name(&self) -> &'static str {
        "signing"
    }

    fn migrations(&self) -> Option<sqlx::migrate::Migrator> {
        Some(sqlx::migrate!())
    }

    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
        ops::register(cx)
    }
}
