//! The `memberships` table.

use sqlx::SqliteExecutor;

use crate::ids::{UserId, WorkspaceId};
use crate::ops::Role;
use crate::time::UnixMillis;

/// A member and their account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemberRow {
    pub(crate) user_id: UserId,
    pub(crate) email: String,
    pub(crate) name: Option<String>,
    pub(crate) role: String,
    pub(crate) created_at: UnixMillis,
}

/// The role of `user_id` in `workspace_id`, or `None` for non-members.
pub(crate) async fn role_of(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    user_id: UserId,
) -> anyhow::Result<Option<Role>> {
    let role = sqlx::query_scalar!(
        "SELECT role FROM memberships WHERE workspace_id = ? AND user_id = ?",
        workspace_id,
        user_id
    )
    .fetch_optional(exec)
    .await?;
    role.as_deref().map(Role::parse).transpose()
}

/// Adds a membership; does nothing when one exists. True when it added one.
pub(crate) async fn insert(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    user_id: UserId,
    role: Role,
    now: UnixMillis,
) -> sqlx::Result<bool> {
    let role = role.as_str();
    let result = sqlx::query!(
        "INSERT INTO memberships (workspace_id, user_id, role, created_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (workspace_id, user_id) DO NOTHING",
        workspace_id,
        user_id,
        role,
        now
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn list(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
) -> sqlx::Result<Vec<MemberRow>> {
    sqlx::query_as!(
        MemberRow,
        r#"SELECT m.user_id AS "user_id: UserId", u.email, u.name, m.role, m.created_at AS "created_at: UnixMillis"
           FROM memberships m JOIN users u ON u.id = m.user_id
           WHERE m.workspace_id = ? ORDER BY u.email"#,
        workspace_id
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn find(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    user_id: UserId,
) -> sqlx::Result<Option<MemberRow>> {
    sqlx::query_as!(
        MemberRow,
        r#"SELECT m.user_id AS "user_id: UserId", u.email, u.name, m.role, m.created_at AS "created_at: UnixMillis"
           FROM memberships m JOIN users u ON u.id = m.user_id
           WHERE m.workspace_id = ? AND m.user_id = ?"#,
        workspace_id,
        user_id
    )
    .fetch_optional(exec)
    .await
}

pub(crate) async fn admin_count(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM memberships WHERE workspace_id = ? AND role = 'admin'"#,
        workspace_id
    )
    .fetch_one(exec)
    .await
}

pub(crate) async fn set_role(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    user_id: UserId,
    role: Role,
) -> sqlx::Result<()> {
    let role = role.as_str();
    sqlx::query!(
        "UPDATE memberships SET role = ? WHERE workspace_id = ? AND user_id = ?",
        role,
        workspace_id,
        user_id
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn remove(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    user_id: UserId,
) -> sqlx::Result<()> {
    sqlx::query!(
        "DELETE FROM memberships WHERE workspace_id = ? AND user_id = ?",
        workspace_id,
        user_id
    )
    .execute(exec)
    .await?;
    Ok(())
}
