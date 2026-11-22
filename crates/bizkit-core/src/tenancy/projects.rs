//! The `projects` table.

use sqlx::SqliteExecutor;

use crate::ids::{ProjectId, WorkspaceId};
use crate::time::UnixMillis;

/// A project row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectRow {
    pub(crate) id: ProjectId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) created_at: UnixMillis,
}

pub(crate) async fn insert(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    slug: &str,
    name: &str,
    now: UnixMillis,
) -> sqlx::Result<ProjectRow> {
    let id = ProjectId::new();
    sqlx::query!(
        "INSERT INTO projects (id, workspace_id, slug, name, created_at) VALUES (?, ?, ?, ?, ?)",
        id,
        workspace_id,
        slug,
        name,
        now
    )
    .execute(exec)
    .await?;
    Ok(ProjectRow {
        id,
        workspace_id,
        slug: slug.to_owned(),
        name: name.to_owned(),
        created_at: now,
    })
}

pub(crate) async fn list(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    limit: i64,
) -> sqlx::Result<Vec<ProjectRow>> {
    sqlx::query_as!(
        ProjectRow,
        r#"SELECT id AS "id: ProjectId", workspace_id AS "workspace_id: WorkspaceId", slug, name,
                  created_at AS "created_at: UnixMillis"
           FROM projects WHERE workspace_id = ? ORDER BY name, id LIMIT ?"#,
        workspace_id,
        limit
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn find(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ProjectId,
) -> sqlx::Result<Option<ProjectRow>> {
    sqlx::query_as!(
        ProjectRow,
        r#"SELECT id AS "id: ProjectId", workspace_id AS "workspace_id: WorkspaceId", slug, name,
                  created_at AS "created_at: UnixMillis"
           FROM projects WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        id
    )
    .fetch_optional(exec)
    .await
}

/// True when `project_id` is a project of `workspace_id`.
///
/// # Errors
///
/// When the query fails.
pub async fn exists(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<bool> {
    let found = sqlx::query_scalar!(
        "SELECT 1 AS found FROM projects WHERE workspace_id = ? AND id = ?",
        workspace_id,
        project_id
    )
    .fetch_optional(exec)
    .await?;
    Ok(found.is_some())
}

pub(crate) async fn rename(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ProjectId,
    name: &str,
) -> sqlx::Result<Option<ProjectRow>> {
    sqlx::query_as!(
        ProjectRow,
        r#"UPDATE projects SET name = ? WHERE workspace_id = ? AND id = ?
           RETURNING id AS "id: ProjectId", workspace_id AS "workspace_id: WorkspaceId", slug, name,
                     created_at AS "created_at: UnixMillis""#,
        name,
        workspace_id,
        id
    )
    .fetch_optional(exec)
    .await
}

pub(crate) async fn delete(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ProjectId,
) -> sqlx::Result<bool> {
    let result =
        sqlx::query!("DELETE FROM projects WHERE workspace_id = ? AND id = ?", workspace_id, id)
            .execute(exec)
            .await?;
    Ok(result.rows_affected() == 1)
}
