//! The bizkit platform: an axum server on SQLite, an operation registry that
//! turns each operation into a JSON route, an OpenAPI entry and an MCP tool,
//! magic-link sign-in, workspaces, API keys and a jobs-table outbox.
//!
//! Feature modules implement [`Module`] and pass to [`App::build`].

mod app;
pub mod auth;
pub mod config;
pub mod contacts;
mod core_module;
pub mod db;
pub mod deps;
#[cfg(feature = "test-hooks")]
pub mod dev;
pub mod error;
pub mod features;
pub mod hooks;
pub mod ids;
pub mod jobs;
pub mod mail;
mod module;
pub mod net;
pub mod ops;
pub mod secrets;
mod tenancy;
#[cfg(feature = "test-hooks")]
pub mod test_hooks;
pub mod time;
pub mod usage;
pub mod validate;

pub use crate::app::{App, Platform, migrators, open_db, openapi, scope_areas};
pub use crate::auth::middleware::Authn;
pub use crate::config::Config;
pub use crate::deps::Deps;
pub use crate::error::ApiError;
pub use crate::module::{DemoSeed, Lifecycle, Module, ModuleContext};

/// Projects, for modules that keep data per project.
pub mod projects {
    pub use crate::tenancy::projects::{exists, find_by_slugs};
}

/// Workspace members, for modules that check an address against them.
pub mod members {
    pub use crate::tenancy::members::is_member_email;
}

/// The API types core's operations return.
pub mod api {
    pub use crate::auth::ops::{ApiKey, ApiKeyList, CreatedApiKey};
    pub use crate::mail::ops::{
        MailSettings, MailTestResult, MailgunSettings, SmtpSettings, WorkspaceMail,
    };
    pub use crate::tenancy::ops::{
        Invitation, InvitationList, Me, Member, MemberList, Membership, MembershipList, Meta,
        MyInvitation, MyInvitationList, Project, ProjectCreated, ProjectDeleted, ProjectList,
        Workspace, WorkspaceRef,
    };
}

#[doc(hidden)]
pub mod __private {
    pub use {serde, sqlx, utoipa, uuid};
}
