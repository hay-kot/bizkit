//! The analytics tables. Every query filters on `workspace_id`, except the
//! site lookup by public key: ingest knows only the key, and later queries
//! use the workspace of the row it returns.

use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::time::UnixMillis;
use sqlx::SqliteExecutor;

use crate::api::GoalId;

/// An `analytics_sites` row. `hostnames` stays JSON text here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SiteRow {
    pub(crate) project_id: ProjectId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) site_key: String,
    pub(crate) hostnames: String,
    pub(crate) identity_mode: String,
    pub(crate) last_event_day: Option<String>,
    pub(crate) rolled_through: Option<String>,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

/// An `analytics_goals` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRow {
    pub(crate) id: GoalId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) matches: String,
    pub(crate) created_at: UnixMillis,
}

pub(crate) async fn insert_site(exec: impl SqliteExecutor<'_>, row: &SiteRow) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO analytics_sites (project_id, workspace_id, site_key, hostnames, identity_mode,
                                      last_event_day, rolled_through, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.project_id,
        row.workspace_id,
        row.site_key,
        row.hostnames,
        row.identity_mode,
        row.last_event_day,
        row.rolled_through,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn find_site(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<Option<SiteRow>> {
    sqlx::query_as!(
        SiteRow,
        r#"SELECT project_id AS "project_id: ProjectId", workspace_id AS "workspace_id: WorkspaceId",
                  site_key, hostnames, identity_mode, last_event_day, rolled_through,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM analytics_sites WHERE workspace_id = ? AND project_id = ?"#,
        workspace_id,
        project_id,
    )
    .fetch_optional(exec)
    .await
}

/// Writes the settings columns of `row`, matched by workspace and project.
pub(crate) async fn update_site_settings(
    exec: impl SqliteExecutor<'_>,
    row: &SiteRow,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE analytics_sites SET hostnames = ?, identity_mode = ?, updated_at = ?
         WHERE workspace_id = ? AND project_id = ?",
        row.hostnames,
        row.identity_mode,
        row.updated_at,
        row.workspace_id,
        row.project_id,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn insert_goal(exec: impl SqliteExecutor<'_>, row: &GoalRow) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO analytics_goals (id, workspace_id, project_id, name, kind, match, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.project_id,
        row.name,
        row.kind,
        row.matches,
        row.created_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn list_goals(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<Vec<GoalRow>> {
    sqlx::query_as!(
        GoalRow,
        r#"SELECT id AS "id: GoalId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, kind, match AS matches,
                  created_at AS "created_at: UnixMillis"
           FROM analytics_goals WHERE workspace_id = ? AND project_id = ?
           ORDER BY name, id"#,
        workspace_id,
        project_id,
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn goal_exists(
    exec: impl SqliteExecutor<'_>,
    project_id: ProjectId,
    kind: &str,
    matches: &str,
) -> sqlx::Result<bool> {
    let found = sqlx::query_scalar!(
        "SELECT 1 AS found FROM analytics_goals WHERE project_id = ? AND kind = ? AND match = ?",
        project_id,
        kind,
        matches,
    )
    .fetch_optional(exec)
    .await?;
    Ok(found.is_some())
}

pub(crate) async fn delete_goal(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    goal_id: GoalId,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM analytics_goals WHERE workspace_id = ? AND id = ?",
        workspace_id,
        goal_id
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}
