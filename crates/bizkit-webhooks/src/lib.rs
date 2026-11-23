//! The webhooks module: capture endpoints per project that store every
//! request they get, a request inspector, forwarding and replay.

pub mod api;
pub mod capture;
mod deliver;
pub mod hooks;
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

use crate::deliver::{Forward, ForwardHandler};
use crate::hooks::{
    ENDPOINT, EndpointTargets, REQUEST_RECEIVED, RequestReceived, WebhooksHookSource,
};

/// The webhooks module.
#[derive(Debug, Clone, Copy, Default)]
pub struct Webhooks;

impl Module for Webhooks {
    fn name(&self) -> &'static str {
        "webhooks"
    }

    fn migrations(&self) -> Option<sqlx::migrate::Migrator> {
        Some(sqlx::migrate!())
    }

    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
        ops::register(cx)?;
        cx.job::<Forward, _>(ForwardHandler)?;
        cx.hook_target_kind(&ENDPOINT, EndpointTargets)?;
        cx.async_hook::<RequestReceived, _>(REQUEST_RECEIVED, WebhooksHookSource)?;
        cx.public_routes(capture::router(), capture::CaptureDocs::openapi())
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
