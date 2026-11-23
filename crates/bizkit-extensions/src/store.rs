//! The extensions tables. Every query filters on `workspace_id`.

use std::collections::BTreeMap;

use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::time::UnixMillis;
use sqlx::SqliteExecutor;

use crate::api::{RunId, ScriptId};

/// Runs kept per script.
pub(crate) const RUNS_KEPT: i64 = 100;

/// An `extension_scripts` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptRow {
    pub(crate) id: ScriptId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) hook: String,
    pub(crate) sync: bool,
    /// The bound entity; `None` for a project-wide script.
    pub(crate) target_id: Option<String>,
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) enabled: bool,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

/// An `extension_runs` row. JSON columns stay text here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunRow {
    pub(crate) id: RunId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) script_id: ScriptId,
    pub(crate) cause: String,
    pub(crate) status: String,
    pub(crate) attempt: Option<i64>,
    pub(crate) duration_ms: i64,
    pub(crate) error: Option<String>,
    pub(crate) logs: String,
    pub(crate) output: Option<String>,
    pub(crate) created_at: UnixMillis,
}

pub(crate) async fn insert_script(
    exec: impl SqliteExecutor<'_>,
    row: &ScriptRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO extension_scripts (id, workspace_id, project_id, hook, sync, target_id, name,
                                        source, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.project_id,
        row.hook,
        row.sync,
        row.target_id,
        row.name,
        row.source,
        row.enabled,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Writes the name, source, enabled flag and `updated_at` of `row`.
pub(crate) async fn update_script(
    exec: impl SqliteExecutor<'_>,
    row: &ScriptRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE extension_scripts SET name = ?, source = ?, enabled = ?, updated_at = ?
         WHERE workspace_id = ? AND id = ?",
        row.name,
        row.source,
        row.enabled,
        row.updated_at,
        row.workspace_id,
        row.id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Binds a project-wide script to an entity. Only the demo seed does this:
/// the API keeps targets fixed after create.
#[cfg(feature = "test-hooks")]
pub(crate) async fn bind_script(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ScriptId,
    target_id: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE extension_scripts SET target_id = ? WHERE workspace_id = ? AND id = ?",
        target_id,
        workspace_id,
        id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn delete_script(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ScriptId,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM extension_scripts WHERE workspace_id = ? AND id = ?",
        workspace_id,
        id
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn find_script(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ScriptId,
) -> sqlx::Result<Option<ScriptRow>> {
    sqlx::query_as!(
        ScriptRow,
        r#"SELECT id AS "id: ScriptId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", hook, sync AS "sync: bool", target_id,
                  name, source,
                  enabled AS "enabled: bool", created_at AS "created_at: UnixMillis",
                  updated_at AS "updated_at: UnixMillis"
           FROM extension_scripts WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        id
    )
    .fetch_optional(exec)
    .await
}

/// The workspace's scripts by name, or one project's, or those bound to
/// one entity.
pub(crate) async fn list_scripts(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: Option<ProjectId>,
    target_id: Option<&str>,
) -> sqlx::Result<Vec<ScriptRow>> {
    sqlx::query_as!(
        ScriptRow,
        r#"SELECT id AS "id: ScriptId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", hook, sync AS "sync: bool", target_id,
                  name, source,
                  enabled AS "enabled: bool", created_at AS "created_at: UnixMillis",
                  updated_at AS "updated_at: UnixMillis"
           FROM extension_scripts
           WHERE workspace_id = ?1 AND (?2 IS NULL OR project_id = ?2)
             AND (?3 IS NULL OR target_id = ?3)
           ORDER BY name, id"#,
        workspace_id,
        project_id,
        target_id
    )
    .fetch_all(exec)
    .await
}

/// The enabled scripts on `hook` for a project that apply to `entity`:
/// the project-wide ones and those bound to it. Oldest first.
pub(crate) async fn enabled_scripts(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    hook: &str,
    entity: Option<&str>,
) -> sqlx::Result<Vec<ScriptRow>> {
    sqlx::query_as!(
        ScriptRow,
        r#"SELECT id AS "id: ScriptId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", hook, sync AS "sync: bool", target_id,
                  name, source,
                  enabled AS "enabled: bool", created_at AS "created_at: UnixMillis",
                  updated_at AS "updated_at: UnixMillis"
           FROM extension_scripts
           WHERE workspace_id = ?1 AND project_id = ?2 AND hook = ?3 AND enabled = 1
             AND (target_id IS NULL OR target_id = ?4)
           ORDER BY id"#,
        workspace_id,
        project_id,
        hook,
        entity
    )
    .fetch_all(exec)
    .await
}

/// Whether the sync point `hook` already has a script for `target_id`, or
/// a project-wide one when it is `None`.
pub(crate) async fn has_sync_script(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    hook: &str,
    target_id: Option<&str>,
) -> sqlx::Result<bool> {
    // `IS`, not `=`: a NULL target must match the project-wide slot.
    let found = sqlx::query_scalar!(
        "SELECT 1 FROM extension_scripts
         WHERE workspace_id = ? AND project_id = ? AND hook = ? AND sync = 1 AND target_id IS ?",
        workspace_id,
        project_id,
        hook,
        target_id
    )
    .fetch_optional(exec)
    .await?;
    Ok(found.is_some())
}

/// Inserts a run and drops the script's runs past the newest [`RUNS_KEPT`].
pub(crate) async fn insert_run(tx: &mut sqlx::SqliteConnection, row: &RunRow) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO extension_runs (id, workspace_id, script_id, cause, status, attempt,
                                     duration_ms, error, logs, output, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.script_id,
        row.cause,
        row.status,
        row.attempt,
        row.duration_ms,
        row.error,
        row.logs,
        row.output,
        row.created_at,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM extension_runs
         WHERE workspace_id = ?1 AND script_id = ?2 AND id NOT IN (
           SELECT id FROM extension_runs WHERE workspace_id = ?1 AND script_id = ?2
           ORDER BY id DESC LIMIT ?3)",
        row.workspace_id,
        row.script_id,
        RUNS_KEPT,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// A script's runs, newest first.
pub(crate) async fn list_runs(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    script_id: ScriptId,
    limit: i64,
) -> sqlx::Result<Vec<RunRow>> {
    sqlx::query_as!(
        RunRow,
        r#"SELECT id AS "id: RunId", workspace_id AS "workspace_id: WorkspaceId",
                  script_id AS "script_id: ScriptId", cause, status, attempt, duration_ms, error,
                  logs, output, created_at AS "created_at: UnixMillis"
           FROM extension_runs WHERE workspace_id = ? AND script_id = ?
           ORDER BY id DESC LIMIT ?"#,
        workspace_id,
        script_id,
        limit
    )
    .fetch_all(exec)
    .await
}

/// A project's secret names and when they were set, by name.
pub(crate) async fn list_secrets(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<Vec<(String, UnixMillis)>> {
    let rows = sqlx::query!(
        r#"SELECT name, updated_at AS "updated_at: UnixMillis" FROM extension_secrets
           WHERE workspace_id = ? AND project_id = ? ORDER BY name"#,
        workspace_id,
        project_id
    )
    .fetch_all(exec)
    .await?;
    Ok(rows.into_iter().map(|row| (row.name, row.updated_at)).collect())
}

/// A project's secrets by name, for a run.
pub(crate) async fn secret_values(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<BTreeMap<String, String>> {
    let rows = sqlx::query!(
        "SELECT name, value FROM extension_secrets WHERE workspace_id = ? AND project_id = ?",
        workspace_id,
        project_id
    )
    .fetch_all(exec)
    .await?;
    Ok(rows.into_iter().map(|row| (row.name, row.value)).collect())
}

/// A secret to write.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SecretRow<'a> {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) name: &'a str,
    pub(crate) value: &'a str,
    pub(crate) updated_at: UnixMillis,
}

pub(crate) async fn put_secret(
    exec: impl SqliteExecutor<'_>,
    row: &SecretRow<'_>,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO extension_secrets (workspace_id, project_id, name, value, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (project_id, name) DO UPDATE SET value = excluded.value,
                                                      updated_at = excluded.updated_at",
        row.workspace_id,
        row.project_id,
        row.name,
        row.value,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn delete_secret(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    name: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM extension_secrets WHERE workspace_id = ? AND project_id = ? AND name = ?",
        workspace_id,
        project_id,
        name
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}
