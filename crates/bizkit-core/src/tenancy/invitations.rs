//! The `invitations` table.

use sqlx::SqliteExecutor;

use crate::ids::{InvitationId, UserId, WorkspaceId};
use crate::ops::Role;
use crate::time::UnixMillis;

/// An invitation row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InvitationRow {
    pub(crate) id: InvitationId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) email: String,
    pub(crate) role: String,
    pub(crate) invited_by: Option<UserId>,
    pub(crate) created_at: UnixMillis,
    pub(crate) expires_at: UnixMillis,
    pub(crate) accepted_at: Option<UnixMillis>,
    pub(crate) revoked_at: Option<UnixMillis>,
}

impl InvitationRow {
    /// Neither accepted nor revoked, and not expired at `now`.
    pub(crate) fn is_open(&self, now: UnixMillis) -> bool {
        self.accepted_at.is_none() && self.revoked_at.is_none() && self.expires_at > now
    }
}

/// An open invitation with its workspace, for the invitee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InviteeView {
    pub(crate) id: InvitationId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) workspace_slug: String,
    pub(crate) workspace_name: String,
    pub(crate) role: String,
    pub(crate) invited_by_email: Option<String>,
    pub(crate) expires_at: UnixMillis,
}

/// The fields of a new invitation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NewInvitation<'a> {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) email: &'a str,
    pub(crate) role: Role,
    pub(crate) invited_by: Option<UserId>,
    pub(crate) expires_at: UnixMillis,
}

pub(crate) async fn insert(
    exec: impl SqliteExecutor<'_>,
    invite: NewInvitation<'_>,
    now: UnixMillis,
) -> sqlx::Result<InvitationRow> {
    let id = InvitationId::new();
    let role = invite.role.as_str();
    sqlx::query!(
        "INSERT INTO invitations (id, workspace_id, email, role, invited_by, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        id,
        invite.workspace_id,
        invite.email,
        role,
        invite.invited_by,
        now,
        invite.expires_at
    )
    .execute(exec)
    .await?;
    Ok(InvitationRow {
        id,
        workspace_id: invite.workspace_id,
        email: invite.email.to_owned(),
        role: role.to_owned(),
        invited_by: invite.invited_by,
        created_at: now,
        expires_at: invite.expires_at,
        accepted_at: None,
        revoked_at: None,
    })
}

pub(crate) async fn find(
    exec: impl SqliteExecutor<'_>,
    id: InvitationId,
) -> sqlx::Result<Option<InvitationRow>> {
    sqlx::query_as!(
        InvitationRow,
        r#"SELECT id AS "id: InvitationId", workspace_id AS "workspace_id: WorkspaceId", email, role,
                  invited_by AS "invited_by: UserId", created_at AS "created_at: UnixMillis",
                  expires_at AS "expires_at: UnixMillis", accepted_at AS "accepted_at: UnixMillis",
                  revoked_at AS "revoked_at: UnixMillis"
           FROM invitations WHERE id = ?"#,
        id
    )
    .fetch_optional(exec)
    .await
}

/// The pending (unaccepted, unrevoked) invitation for `email`, expired or not.
pub(crate) async fn find_pending(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    email: &str,
) -> sqlx::Result<Option<InvitationRow>> {
    sqlx::query_as!(
        InvitationRow,
        r#"SELECT id AS "id: InvitationId", workspace_id AS "workspace_id: WorkspaceId", email, role,
                  invited_by AS "invited_by: UserId", created_at AS "created_at: UnixMillis",
                  expires_at AS "expires_at: UnixMillis", accepted_at AS "accepted_at: UnixMillis",
                  revoked_at AS "revoked_at: UnixMillis"
           FROM invitations
           WHERE workspace_id = ? AND email = ? AND accepted_at IS NULL AND revoked_at IS NULL"#,
        workspace_id,
        email
    )
    .fetch_optional(exec)
    .await
}

/// Pending invitations of a workspace, newest first.
pub(crate) async fn list_pending(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
) -> sqlx::Result<Vec<InvitationRow>> {
    sqlx::query_as!(
        InvitationRow,
        r#"SELECT id AS "id: InvitationId", workspace_id AS "workspace_id: WorkspaceId", email, role,
                  invited_by AS "invited_by: UserId", created_at AS "created_at: UnixMillis",
                  expires_at AS "expires_at: UnixMillis", accepted_at AS "accepted_at: UnixMillis",
                  revoked_at AS "revoked_at: UnixMillis"
           FROM invitations
           WHERE workspace_id = ? AND accepted_at IS NULL AND revoked_at IS NULL
           ORDER BY created_at DESC, id"#,
        workspace_id
    )
    .fetch_all(exec)
    .await
}

/// True when `email` has an open invitation to any workspace.
pub(crate) async fn has_open(
    exec: impl SqliteExecutor<'_>,
    email: &str,
    now: UnixMillis,
) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM invitations
             WHERE email = ? AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > ?
           ) AS "open!: bool""#,
        email,
        now
    )
    .fetch_one(exec)
    .await
}

/// Open invitations for `email`, with their workspaces.
pub(crate) async fn list_open_for_email(
    exec: impl SqliteExecutor<'_>,
    email: &str,
    now: UnixMillis,
) -> sqlx::Result<Vec<InviteeView>> {
    sqlx::query_as!(
        InviteeView,
        r#"SELECT i.id AS "id: InvitationId", w.id AS "workspace_id: WorkspaceId", w.slug AS workspace_slug,
                  w.name AS workspace_name, i.role, u.email AS "invited_by_email?",
                  i.expires_at AS "expires_at: UnixMillis"
           FROM invitations i
           JOIN workspaces w ON w.id = i.workspace_id
           LEFT JOIN users u ON u.id = i.invited_by
           WHERE i.email = ? AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at > ?
           ORDER BY i.created_at DESC, i.id"#,
        email,
        now
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn mark_accepted(
    exec: impl SqliteExecutor<'_>,
    id: InvitationId,
    now: UnixMillis,
) -> sqlx::Result<()> {
    sqlx::query!("UPDATE invitations SET accepted_at = ? WHERE id = ?", now, id)
        .execute(exec)
        .await?;
    Ok(())
}

pub(crate) async fn mark_revoked(
    exec: impl SqliteExecutor<'_>,
    id: InvitationId,
    now: UnixMillis,
) -> sqlx::Result<()> {
    sqlx::query!("UPDATE invitations SET revoked_at = ? WHERE id = ?", now, id)
        .execute(exec)
        .await?;
    Ok(())
}
