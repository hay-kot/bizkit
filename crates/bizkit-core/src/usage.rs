//! bizkit's own product usage, announced so the analytics module can send
//! it to the project an operator names. Core never depends on analytics.
//!
//! [`UsageKind`] is the whole list of what bizkit reports about its users:
//! a user ID, one of these names, the workspace ID, and the counts in
//! [`UsageTraits`]. No emails, names or slugs.

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::db::WriteTx;
use crate::ids::{UserId, WorkspaceId};
use crate::jobs::Event;
use crate::ops::{OpCtx, Principal};
use crate::time::UnixMillis;

/// What a user did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageKind {
    /// Signed in for the first time, which made the account.
    SignedUp,
    /// Signed in to an existing account.
    SignedIn,
    /// Created a workspace.
    WorkspaceCreated,
    /// Created a project.
    ProjectCreated,
    /// Invited someone to a workspace.
    InvitationSent,
    /// Joined a workspace by accepting an invitation.
    MemberJoined,
    /// Created an API key.
    ApiKeyCreated,
    /// Revoked an API key.
    ApiKeyRevoked,
}

impl UsageKind {
    /// The analytics event name.
    #[must_use]
    pub const fn event_name(self) -> &'static str {
        match self {
            Self::SignedUp => "signed_up",
            Self::SignedIn => "signed_in",
            Self::WorkspaceCreated => "workspace_created",
            Self::ProjectCreated => "project_created",
            Self::InvitationSent => "invitation_sent",
            Self::MemberJoined => "member_joined",
            Self::ApiKeyCreated => "api_key_created",
            Self::ApiKeyRevoked => "api_key_revoked",
        }
    }
}

/// Published in the transaction of a user's action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageReported {
    /// Who acted.
    pub user_id: UserId,
    /// What they did.
    pub kind: UsageKind,
    /// The workspace they did it in, if any.
    pub workspace_id: Option<WorkspaceId>,
}

impl Event for UsageReported {
    const NAME: &'static str = "usage.reported";
}

/// A user's current state, sent as person traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UsageTraits {
    /// Workspaces the user belongs to.
    pub workspaces: i64,
    /// API keys the user created that still work.
    pub api_keys: i64,
}

/// The user's traits now. Read when the report is sent, not when the action
/// happened, so a retried or late report still sends current counts.
///
/// # Errors
///
/// When a query fails.
pub async fn traits(db: &SqlitePool, user: UserId, now: UnixMillis) -> anyhow::Result<UsageTraits> {
    let workspaces = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM memberships WHERE user_id = ?"#,
        user,
    )
    .fetch_one(db)
    .await
    .context("counting a user's workspaces")?;
    let api_keys = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM api_keys
           WHERE created_by = ? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)"#,
        user,
        now,
    )
    .fetch_one(db)
    .await
    .context("counting a user's API keys")?;
    Ok(UsageTraits { workspaces, api_keys })
}

/// Publishes a [`UsageReported`] in `tx`. Nothing for API keys and anonymous
/// calls: they have no person to count the action for.
pub(crate) async fn report(
    cx: &OpCtx,
    tx: &mut WriteTx,
    kind: UsageKind,
    workspace_id: Option<WorkspaceId>,
) -> anyhow::Result<()> {
    let Some(Principal::User(user)) = cx.principal() else { return Ok(()) };
    let event = UsageReported { user_id: user.user_id, kind, workspace_id };
    cx.platform().bus().publish(tx, &event).await
}
