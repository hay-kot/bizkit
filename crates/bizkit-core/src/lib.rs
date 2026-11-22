//! The bizkit platform: an axum server on SQLite, an operation registry that
//! turns each operation into a JSON route, an OpenAPI entry and an MCP tool,
//! magic-link sign-in, workspaces, API keys and a jobs-table outbox.
//!
//! Feature modules implement [`Module`] and pass to [`App::build`].

mod app;
pub mod auth;
pub mod config;
mod core_module;
pub mod db;
pub mod deps;
#[cfg(feature = "test-hooks")]
pub mod dev;
pub mod error;
pub mod ids;
pub mod jobs;
mod module;
pub mod ops;
mod tenancy;
#[cfg(feature = "test-hooks")]
pub mod test_hooks;
pub mod time;
pub mod validate;

pub use crate::app::{App, Platform, migrators, open_db, openapi, scope_areas};
pub use crate::auth::middleware::Authn;
pub use crate::config::Config;
pub use crate::deps::Deps;
pub use crate::error::ApiError;
pub use crate::module::{DemoSeed, Module, ModuleContext};

/// Projects, for modules that keep data per project.
pub mod projects {
    pub use crate::tenancy::projects::exists;
}

/// The API types core's operations return.
pub mod api {
    pub use crate::auth::ops::{ApiKey, ApiKeyList, CreatedApiKey};
    pub use crate::tenancy::ops::{
        Invitation, InvitationList, Me, Member, MemberList, Membership, MembershipList, Meta,
        MyInvitation, MyInvitationList, Project, ProjectCreated, ProjectList, Workspace,
        WorkspaceRef,
    };
}

#[doc(hidden)]
pub mod __private {
    pub use {serde, sqlx, utoipa, uuid};
}
