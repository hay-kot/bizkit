//! Core's own features, registered like any module.

use crate::jobs::mail::SendMailHandler;
use crate::module::{Module, ModuleContext};

/// Auth, tenancy and the `mail.send` job.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CoreModule;

impl Module for CoreModule {
    fn name(&self) -> &'static str {
        "core"
    }

    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
        crate::tenancy::ops::register(cx)?;
        crate::auth::ops::register(cx)?;
        cx.job(SendMailHandler)?;
        Ok(())
    }
}
