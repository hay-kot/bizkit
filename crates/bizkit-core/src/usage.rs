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
use crate::jobs::{Bus, Event};
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
    /// Removed someone from a workspace.
    MemberRemoved,
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
            Self::MemberRemoved => "member_removed",
            Self::ApiKeyCreated => "api_key_created",
            Self::ApiKeyRevoked => "api_key_revoked",
        }
    }
}

/// Published in the transaction of a user's action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageReported {
    /// Who acted. `None` when an API key acted: there is no person to
    /// count the event for, and only `affected` users get fresh traits.
    pub user_id: Option<UserId>,
    /// What they did.
    pub kind: UsageKind,
    /// The workspace they did it in, if any.
    pub workspace_id: Option<WorkspaceId>,
    /// When they did it.
    pub at: UnixMillis,
    /// Other users whose traits the action changed, such as the creator of a
    /// key someone else revoked. They get fresh traits, not the event.
    #[serde(default)]
    pub affected: Vec<UserId>,
}

impl Event for UsageReported {
    const NAME: &'static str = "usage.reported";
}

/// A user's current state, sent as person traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UsageTraits {
    /// Workspaces the user belongs to.
    pub workspaces: i64,
    /// API keys the user created and nobody revoked. Expired keys still
    /// count: nothing reports when a key expires, so a count that drops on
    /// expiry would go stale.
    pub api_keys: i64,
}

/// The user's traits now. Read when the report is sent, not when the action
/// happened, so a retried or late report still sends current counts.
///
/// # Errors
///
/// When a query fails.
pub async fn traits(db: &SqlitePool, user: UserId) -> anyhow::Result<UsageTraits> {
    let workspaces = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM memberships WHERE user_id = ?"#,
        user,
    )
    .fetch_one(db)
    .await
    .context("counting a user's workspaces")?;
    let api_keys = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM api_keys
           WHERE created_by = ? AND revoked_at IS NULL"#,
        user,
    )
    .fetch_one(db)
    .await
    .context("counting a user's API keys")?;
    Ok(UsageTraits { workspaces, api_keys })
}

/// An operation's usage report, before [`report`] adds who and when.
#[derive(Debug, Clone)]
pub(crate) struct Report {
    kind: UsageKind,
    workspace_id: WorkspaceId,
    affected: Option<UserId>,
}

impl Report {
    pub(crate) const fn new(kind: UsageKind, workspace_id: WorkspaceId) -> Self {
        Self { kind, workspace_id, affected: None }
    }

    /// Also refreshes `user`'s traits, when the action changed them.
    pub(crate) const fn affecting(mut self, user: Option<UserId>) -> Self {
        self.affected = user;
        self
    }
}

/// Publishes `report` in `tx`. An API key or anonymous caller counts no
/// event, but the users it affected still get fresh traits.
pub(crate) async fn report(cx: &OpCtx, tx: &mut WriteTx, report: Report) -> anyhow::Result<()> {
    let user_id = match cx.principal() {
        Some(Principal::User(user)) => Some(user.user_id),
        _ => None,
    };
    let affected: Vec<UserId> =
        report.affected.into_iter().filter(|id| Some(*id) != user_id).collect();
    if user_id.is_none() && affected.is_empty() {
        return Ok(());
    }
    let event = UsageReported {
        user_id,
        kind: report.kind,
        workspace_id: Some(report.workspace_id),
        at: cx.now(),
        affected,
    };
    publish(cx.platform().bus(), tx, &event).await
}

/// Publishes `event` in `tx`.
pub(crate) async fn publish(
    bus: &Bus,
    tx: &mut WriteTx,
    event: &UsageReported,
) -> anyhow::Result<()> {
    bus.publish(tx, event).await.with_context(|| format!("reporting {}", event.kind.event_name()))
}
