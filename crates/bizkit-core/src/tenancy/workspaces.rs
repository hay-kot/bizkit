//! The `workspaces` table.

use sqlx::SqliteExecutor;

use crate::ids::{UserId, WorkspaceId};
use crate::ops::Role;
use crate::time::UnixMillis;

/// A workspace row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceRow {
    pub(crate) id: WorkspaceId,
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) created_at: UnixMillis,
}

/// A workspace and the caller's role in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemberWorkspace {
    pub(crate) id: WorkspaceId,
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) created_at: UnixMillis,
    pub(crate) role: String,
}

impl MemberWorkspace {
    pub(crate) fn split(self) -> anyhow::Result<(WorkspaceRow, Role)> {
        let role = Role::parse(&self.role)?;
        Ok((
            WorkspaceRow {
                id: self.id,
                slug: self.slug,
                name: self.name,
                created_at: self.created_at,
            },
            role,
        ))
    }
}

pub(crate) async fn insert(
    exec: impl SqliteExecutor<'_>,
    slug: &str,
    name: &str,
    now: UnixMillis,
) -> sqlx::Result<WorkspaceRow> {
    let id = WorkspaceId::new();
    sqlx::query!(
        "INSERT INTO workspaces (id, slug, name, created_at) VALUES (?, ?, ?, ?)",
        id,
        slug,
        name,
        now
    )
    .execute(exec)
    .await?;
    Ok(WorkspaceRow { id, slug: slug.to_owned(), name: name.to_owned(), created_at: now })
}

pub(crate) async fn find(
    exec: impl SqliteExecutor<'_>,
    id: WorkspaceId,
) -> sqlx::Result<Option<WorkspaceRow>> {
    sqlx::query_as!(
        WorkspaceRow,
        r#"SELECT id AS "id: WorkspaceId", slug, name, created_at AS "created_at: UnixMillis"
           FROM workspaces WHERE id = ?"#,
        id
    )
    .fetch_optional(exec)
    .await
}

pub(crate) async fn find_by_slug(
    exec: impl SqliteExecutor<'_>,
    slug: &str,
) -> sqlx::Result<Option<WorkspaceRow>> {
    sqlx::query_as!(
        WorkspaceRow,
        r#"SELECT id AS "id: WorkspaceId", slug, name, created_at AS "created_at: UnixMillis"
           FROM workspaces WHERE slug = ?"#,
        slug
    )
    .fetch_optional(exec)
    .await
}

/// Every workspace `user_id` belongs to, with their role.
pub(crate) async fn list_for_user(
    exec: impl SqliteExecutor<'_>,
    user_id: UserId,
) -> sqlx::Result<Vec<MemberWorkspace>> {
    sqlx::query_as!(
        MemberWorkspace,
        r#"SELECT w.id AS "id: WorkspaceId", w.slug, w.name, w.created_at AS "created_at: UnixMillis", m.role
           FROM workspaces w JOIN memberships m ON m.workspace_id = w.id
           WHERE m.user_id = ? ORDER BY w.name, w.id"#,
        user_id
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn rename(
    exec: impl SqliteExecutor<'_>,
    id: WorkspaceId,
    name: &str,
) -> sqlx::Result<Option<WorkspaceRow>> {
    sqlx::query_as!(
        WorkspaceRow,
        r#"UPDATE workspaces SET name = ? WHERE id = ?
           RETURNING id AS "id: WorkspaceId", slug, name, created_at AS "created_at: UnixMillis""#,
        name,
        id
    )
    .fetch_optional(exec)
    .await
}
