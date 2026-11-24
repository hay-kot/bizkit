//! The extensions tables. Every query filters on `workspace_id`.

use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::time::UnixMillis;
use sqlx::SqliteExecutor;

use crate::api::{RunId, ScriptId, TestCase};

/// Runs kept per script.
pub(crate) const RUNS_KEPT: i64 = 100;

/// An `extension_scripts` row with its target set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptRow {
    pub(crate) id: ScriptId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) hook: String,
    pub(crate) sync: bool,
    /// The entities it runs for, sorted; empty: every entity of the kind.
    pub(crate) target_ids: Vec<String>,
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) tests: Vec<TestCase>,
    pub(crate) enabled: bool,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

/// A script as queried: the target set is a JSON array from
/// `json_group_array`.
struct RawScript {
    id: ScriptId,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    hook: String,
    sync: bool,
    targets: String,
    name: String,
    source: String,
    tests: String,
    enabled: bool,
    created_at: UnixMillis,
    updated_at: UnixMillis,
}

impl TryFrom<RawScript> for ScriptRow {
    type Error = sqlx::Error;

    fn try_from(raw: RawScript) -> sqlx::Result<Self> {
        let decode = |err: serde_json::Error| sqlx::Error::Decode(Box::new(err));
        let target_ids = serde_json::from_str(&raw.targets).map_err(decode)?;
        let tests = serde_json::from_str(&raw.tests).map_err(decode)?;
        Ok(Self {
            id: raw.id,
            workspace_id: raw.workspace_id,
            project_id: raw.project_id,
            hook: raw.hook,
            sync: raw.sync,
            target_ids,
            name: raw.name,
            source: raw.source,
            tests,
            enabled: raw.enabled,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
        })
    }
}

fn rows(raw: Vec<RawScript>) -> sqlx::Result<Vec<ScriptRow>> {
    raw.into_iter().map(ScriptRow::try_from).collect()
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

/// Inserts the script and its target set.
pub(crate) async fn insert_script(
    tx: &mut sqlx::SqliteConnection,
    row: &ScriptRow,
) -> sqlx::Result<()> {
    let tests = encode_tests(&row.tests)?;
    sqlx::query!(
        "INSERT INTO extension_scripts (id, workspace_id, project_id, hook, sync, name, source,
                                        tests, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.project_id,
        row.hook,
        row.sync,
        row.name,
        row.source,
        tests,
        row.enabled,
        row.created_at,
        row.updated_at,
    )
    .execute(&mut *tx)
    .await?;
    insert_targets(tx, row).await
}

fn encode_tests(tests: &[TestCase]) -> sqlx::Result<String> {
    serde_json::to_string(tests).map_err(|err| sqlx::Error::Encode(Box::new(err)))
}

/// Writes the name, source, tests, enabled flag, target set and
/// `updated_at` of `row`.
pub(crate) async fn update_script(
    tx: &mut sqlx::SqliteConnection,
    row: &ScriptRow,
) -> sqlx::Result<()> {
    let tests = encode_tests(&row.tests)?;
    sqlx::query!(
        "UPDATE extension_scripts SET name = ?, source = ?, tests = ?, enabled = ?, updated_at = ?
         WHERE workspace_id = ? AND id = ?",
        row.name,
        row.source,
        tests,
        row.enabled,
        row.updated_at,
        row.workspace_id,
        row.id,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM extension_script_targets WHERE workspace_id = ? AND script_id = ?",
        row.workspace_id,
        row.id,
    )
    .execute(&mut *tx)
    .await?;
    insert_targets(tx, row).await
}

async fn insert_targets(tx: &mut sqlx::SqliteConnection, row: &ScriptRow) -> sqlx::Result<()> {
    for target_id in &row.target_ids {
        sqlx::query!(
            "INSERT INTO extension_script_targets (workspace_id, script_id, target_id)
             VALUES (?, ?, ?)",
            row.workspace_id,
            row.id,
            target_id,
        )
        .execute(&mut *tx)
        .await?;
    }
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
        RawScript,
        r#"SELECT s.id AS "id: ScriptId", s.workspace_id AS "workspace_id: WorkspaceId",
                  s.project_id AS "project_id: ProjectId", s.hook, s.sync AS "sync: bool",
                  (SELECT json_group_array(target_id) FROM (
                     SELECT target_id FROM extension_script_targets t
                     WHERE t.workspace_id = s.workspace_id AND t.script_id = s.id
                     ORDER BY target_id)) AS "targets!: String",
                  s.name, s.source, s.tests, s.enabled AS "enabled: bool",
                  s.created_at AS "created_at: UnixMillis", s.updated_at AS "updated_at: UnixMillis"
           FROM extension_scripts s WHERE s.workspace_id = ? AND s.id = ?"#,
        workspace_id,
        id
    )
    .fetch_optional(exec)
    .await?
    .map(ScriptRow::try_from)
    .transpose()
}

/// The workspace's scripts by name, or one project's.
pub(crate) async fn list_scripts(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: Option<ProjectId>,
) -> sqlx::Result<Vec<ScriptRow>> {
    let raw = sqlx::query_as!(
        RawScript,
        r#"SELECT s.id AS "id: ScriptId", s.workspace_id AS "workspace_id: WorkspaceId",
                  s.project_id AS "project_id: ProjectId", s.hook, s.sync AS "sync: bool",
                  (SELECT json_group_array(target_id) FROM (
                     SELECT target_id FROM extension_script_targets t
                     WHERE t.workspace_id = s.workspace_id AND t.script_id = s.id
                     ORDER BY target_id)) AS "targets!: String",
                  s.name, s.source, s.tests, s.enabled AS "enabled: bool",
                  s.created_at AS "created_at: UnixMillis", s.updated_at AS "updated_at: UnixMillis"
           FROM extension_scripts s
           WHERE s.workspace_id = ?1 AND (?2 IS NULL OR s.project_id = ?2)
           ORDER BY s.name, s.id"#,
        workspace_id,
        project_id,
    )
    .fetch_all(exec)
    .await?;
    rows(raw)
}

/// The enabled scripts on `hook` for a project that apply to `entity`:
/// those with no targets and those whose set names it. Oldest first.
pub(crate) async fn enabled_scripts(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    hook: &str,
    entity: Option<&str>,
) -> sqlx::Result<Vec<ScriptRow>> {
    let raw = sqlx::query_as!(
        RawScript,
        r#"SELECT s.id AS "id: ScriptId", s.workspace_id AS "workspace_id: WorkspaceId",
                  s.project_id AS "project_id: ProjectId", s.hook, s.sync AS "sync: bool",
                  (SELECT json_group_array(target_id) FROM (
                     SELECT target_id FROM extension_script_targets t
                     WHERE t.workspace_id = s.workspace_id AND t.script_id = s.id
                     ORDER BY target_id)) AS "targets!: String",
                  s.name, s.source, s.tests, s.enabled AS "enabled: bool",
                  s.created_at AS "created_at: UnixMillis", s.updated_at AS "updated_at: UnixMillis"
           FROM extension_scripts s
           WHERE s.workspace_id = ?1 AND s.project_id = ?2 AND s.hook = ?3 AND s.enabled = 1
             AND (NOT EXISTS (SELECT 1 FROM extension_script_targets t
                              WHERE t.workspace_id = ?1 AND t.script_id = s.id)
                  OR EXISTS (SELECT 1 FROM extension_script_targets t
                             WHERE t.workspace_id = ?1 AND t.script_id = s.id
                               AND t.target_id = ?4))
           ORDER BY s.id"#,
        workspace_id,
        project_id,
        hook,
        entity
    )
    .fetch_all(exec)
    .await?;
    rows(raw)
}

/// Another sync script that holds a slot `script` wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncConflict {
    /// The other script's name.
    pub(crate) name: String,
    /// The entity both name; `None`: both run for every entity.
    pub(crate) target_id: Option<String>,
}

/// The first other sync script on the same point and project that names
/// one of `script`'s targets, or, when `script` has none, another one with
/// none. Callers run it in the write transaction, which serializes writers.
pub(crate) async fn sync_conflict(
    exec: impl SqliteExecutor<'_>,
    script: &ScriptRow,
) -> sqlx::Result<Option<SyncConflict>> {
    if script.target_ids.is_empty() {
        let found = sqlx::query_scalar!(
            "SELECT s.name FROM extension_scripts s
             WHERE s.workspace_id = ?1 AND s.project_id = ?2 AND s.hook = ?3 AND s.sync = 1
               AND s.id != ?4
               AND NOT EXISTS (SELECT 1 FROM extension_script_targets t
                               WHERE t.workspace_id = ?1 AND t.script_id = s.id)
             ORDER BY s.id LIMIT 1",
            script.workspace_id,
            script.project_id,
            script.hook,
            script.id,
        )
        .fetch_optional(exec)
        .await?;
        return Ok(found.map(|name| SyncConflict { name, target_id: None }));
    }
    let targets = serde_json::to_string(&script.target_ids)
        .map_err(|err| sqlx::Error::Encode(Box::new(err)))?;
    let found = sqlx::query!(
        "SELECT s.name, t.target_id FROM extension_scripts s
         JOIN extension_script_targets t ON t.workspace_id = s.workspace_id AND t.script_id = s.id
         WHERE s.workspace_id = ?1 AND s.project_id = ?2 AND s.hook = ?3 AND s.sync = 1
           AND s.id != ?4 AND t.target_id IN (SELECT value FROM json_each(?5))
         ORDER BY s.id, t.target_id LIMIT 1",
        script.workspace_id,
        script.project_id,
        script.hook,
        script.id,
        targets,
    )
    .fetch_optional(exec)
    .await?;
    Ok(found.map(|row| SyncConflict { name: row.name, target_id: Some(row.target_id) }))
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

/// An `extension_secrets` row. Exactly one of `value` (plaintext, from
/// before secrets were sealed) and `sealed` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredSecret {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) name: String,
    pub(crate) value: Option<String>,
    pub(crate) sealed: Option<Vec<u8>>,
    pub(crate) updated_at: UnixMillis,
}

/// A project's secrets, by name.
pub(crate) async fn project_secrets(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<Vec<StoredSecret>> {
    sqlx::query_as!(
        StoredSecret,
        r#"SELECT workspace_id AS "workspace_id: WorkspaceId", project_id AS "project_id: ProjectId",
                  name, value, sealed, updated_at AS "updated_at: UnixMillis"
           FROM extension_secrets WHERE workspace_id = ? AND project_id = ? ORDER BY name"#,
        workspace_id,
        project_id
    )
    .fetch_all(exec)
    .await
}

/// Every plaintext secret of the instance. Startup seals them across all
/// workspaces at once, so this is the one query that doesn't filter on
/// `workspace_id`; each row carries its own.
pub(crate) async fn plaintext_secrets(
    exec: impl SqliteExecutor<'_>,
) -> sqlx::Result<Vec<StoredSecret>> {
    sqlx::query_as!(
        StoredSecret,
        r#"SELECT workspace_id AS "workspace_id: WorkspaceId", project_id AS "project_id: ProjectId",
                  name, value, sealed, updated_at AS "updated_at: UnixMillis"
           FROM extension_secrets WHERE sealed IS NULL"#
    )
    .fetch_all(exec)
    .await
}

/// Replaces a plaintext value with its sealed form.
pub(crate) async fn seal_secret(
    exec: impl SqliteExecutor<'_>,
    row: &StoredSecret,
    sealed: &[u8],
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE extension_secrets SET value = NULL, sealed = ?
         WHERE workspace_id = ? AND project_id = ? AND name = ? AND sealed IS NULL",
        sealed,
        row.workspace_id,
        row.project_id,
        row.name,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// A secret to write.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SecretRow<'a> {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) name: &'a str,
    pub(crate) sealed: &'a [u8],
    pub(crate) updated_at: UnixMillis,
}

pub(crate) async fn put_secret(
    exec: impl SqliteExecutor<'_>,
    row: &SecretRow<'_>,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO extension_secrets (workspace_id, project_id, name, sealed, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (project_id, name) DO UPDATE SET value = NULL, sealed = excluded.sealed,
                                                      updated_at = excluded.updated_at",
        row.workspace_id,
        row.project_id,
        row.name,
        row.sealed,
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
