//! The doc signing module: a PDF goes out for signature with fields placed
//! on its pages, signers sign in signing-order groups on a public page
//! through a private link, and bizkit seals the final PDF with the
//! signatures and an audit trail.

pub mod api;
mod complete;
pub mod fields;
mod mail;
mod ops;
mod overlay;
mod pdf;
mod public;
mod seal;
#[cfg(feature = "test-hooks")]
mod seed;
mod signature;
mod store;
pub mod testing;
mod upload;

#[cfg(feature = "test-hooks")]
use bizkit_core::DemoSeed;
use bizkit_core::{Module, ModuleContext};
#[cfg(feature = "test-hooks")]
use futures::future::BoxFuture;
use utoipa::OpenApi as _;

/// The demo seed's sender.
#[cfg(feature = "test-hooks")]
const DEMO_SENDER: &str = "demo@bizkit.test";

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
        ops::register(cx)?;
        cx.job::<complete::Complete, _>(complete::CompleteHandler)?;
        cx.public_routes(public::router(), public::PublicDocs::openapi())?;
        cx.upload_routes(upload::upload_router(), upload::UploadDocs::openapi())?;
        cx.routes(upload::file_router(), upload::FileDocs::openapi())
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
