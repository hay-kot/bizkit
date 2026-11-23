//! The forms tables. Every query filters on `workspace_id`, except
//! [`find_public`]: public ingest knows only the form ID, and later queries
//! use the workspace of the row it returns.

use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::time::UnixMillis;
use sqlx::SqliteExecutor;

use crate::api::{FormId, SubmissionId};

/// A `forms` row. JSON columns stay text here; `ops` decodes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormRow {
    pub(crate) id: FormId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) fields: String,
    pub(crate) bot_check: String,
    pub(crate) min_fill_ms: i64,
    pub(crate) redirect_url: Option<String>,
    pub(crate) notify_emails: String,
    pub(crate) challenge_key: String,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

/// A form row and its inbox count, for list and get.
#[derive(Debug, Clone)]
pub(crate) struct FormWithCount {
    pub(crate) form: FormRow,
    pub(crate) inbox_count: i64,
}

/// A `form_submissions` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubmissionRow {
    pub(crate) id: SubmissionId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) form_id: FormId,
    pub(crate) status: String,
    pub(crate) spam_reason: Option<String>,
    pub(crate) data: String,
    pub(crate) user_agent: Option<String>,
    pub(crate) referer: Option<String>,
    pub(crate) created_at: UnixMillis,
}

pub(crate) async fn insert_form(exec: impl SqliteExecutor<'_>, row: &FormRow) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO forms (id, workspace_id, project_id, name, enabled, fields, bot_check, min_fill_ms,
                            redirect_url, notify_emails, challenge_key, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.project_id,
        row.name,
        row.enabled,
        row.fields,
        row.bot_check,
        row.min_fill_ms,
        row.redirect_url,
        row.notify_emails,
        row.challenge_key,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Writes every settings column of `row`, matched by workspace and ID.
pub(crate) async fn update_form(
    exec: impl SqliteExecutor<'_>,
    row: &FormRow,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE forms SET name = ?, enabled = ?, fields = ?, bot_check = ?, min_fill_ms = ?,
                          redirect_url = ?, notify_emails = ?, updated_at = ?
         WHERE workspace_id = ? AND id = ?",
        row.name,
        row.enabled,
        row.fields,
        row.bot_check,
        row.min_fill_ms,
        row.redirect_url,
        row.notify_emails,
        row.updated_at,
        row.workspace_id,
        row.id,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn list_forms(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: Option<ProjectId>,
) -> sqlx::Result<Vec<FormWithCount>> {
    let rows = sqlx::query!(
        r#"SELECT f.id AS "id: FormId", f.workspace_id AS "workspace_id: WorkspaceId",
                  f.project_id AS "project_id: ProjectId", f.name, f.enabled AS "enabled: bool",
                  f.fields, f.bot_check, f.min_fill_ms, f.redirect_url, f.notify_emails,
                  f.challenge_key, f.created_at AS "created_at: UnixMillis",
                  f.updated_at AS "updated_at: UnixMillis",
                  (SELECT COUNT(*) FROM form_submissions s
                   WHERE s.workspace_id = f.workspace_id AND s.form_id = f.id AND s.status = 'inbox')
                    AS "inbox_count!: i64"
           FROM forms f
           WHERE f.workspace_id = ? AND (?2 IS NULL OR f.project_id = ?2)
           ORDER BY f.name, f.id"#,
        workspace_id,
        project_id,
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| FormWithCount {
            form: FormRow {
                id: r.id,
                workspace_id: r.workspace_id,
                project_id: r.project_id,
                name: r.name,
                enabled: r.enabled,
                fields: r.fields,
                bot_check: r.bot_check,
                min_fill_ms: r.min_fill_ms,
                redirect_url: r.redirect_url,
                notify_emails: r.notify_emails,
                challenge_key: r.challenge_key,
                created_at: r.created_at,
                updated_at: r.updated_at,
            },
            inbox_count: r.inbox_count,
        })
        .collect())
}

pub(crate) async fn find_form(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: FormId,
) -> sqlx::Result<Option<FormRow>> {
    sqlx::query_as!(
        FormRow,
        r#"SELECT id AS "id: FormId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, enabled AS "enabled: bool", fields,
                  bot_check, min_fill_ms, redirect_url, notify_emails, challenge_key,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM forms WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        id
    )
    .fetch_optional(exec)
    .await
}

/// The workspace's forms among `ids`, in any order.
pub(crate) async fn find_forms(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    ids: &[FormId],
) -> sqlx::Result<Vec<FormRow>> {
    let ids = serde_json::to_string(ids).map_err(|err| sqlx::Error::Encode(Box::new(err)))?;
    sqlx::query_as!(
        FormRow,
        r#"SELECT id AS "id: FormId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, enabled AS "enabled: bool", fields,
                  bot_check, min_fill_ms, redirect_url, notify_emails, challenge_key,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM forms
           WHERE workspace_id = ? AND id IN (SELECT value FROM json_each(?))"#,
        workspace_id,
        ids
    )
    .fetch_all(exec)
    .await
}

/// The enabled form with `id` in any workspace, for public ingest.
pub(crate) async fn find_public(
    exec: impl SqliteExecutor<'_>,
    id: FormId,
) -> sqlx::Result<Option<FormRow>> {
    sqlx::query_as!(
        FormRow,
        r#"SELECT id AS "id: FormId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, enabled AS "enabled: bool", fields,
                  bot_check, min_fill_ms, redirect_url, notify_emails, challenge_key,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM forms WHERE id = ? AND enabled = 1"#,
        id
    )
    .fetch_optional(exec)
    .await
}

pub(crate) async fn inbox_count(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    form_id: FormId,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM form_submissions
           WHERE workspace_id = ? AND form_id = ? AND status = 'inbox'"#,
        workspace_id,
        form_id
    )
    .fetch_one(exec)
    .await
}

pub(crate) async fn delete_form(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: FormId,
) -> sqlx::Result<bool> {
    let result =
        sqlx::query!("DELETE FROM forms WHERE workspace_id = ? AND id = ?", workspace_id, id)
            .execute(exec)
            .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn insert_submission(
    exec: impl SqliteExecutor<'_>,
    row: &SubmissionRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO form_submissions (id, workspace_id, form_id, status, spam_reason, data,
                                       user_agent, referer, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.form_id,
        row.status,
        row.spam_reason,
        row.data,
        row.user_agent,
        row.referer,
        row.created_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Which submissions [`list_submissions`] returns.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Page<'a> {
    pub(crate) status: &'a str,
    /// Only submissions older than this one.
    pub(crate) before: Option<SubmissionId>,
    pub(crate) limit: i64,
}

/// Submissions newest first.
pub(crate) async fn list_submissions(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    form_id: FormId,
    page: Page<'_>,
) -> sqlx::Result<Vec<SubmissionRow>> {
    let Page { status, before, limit } = page;
    sqlx::query_as!(
        SubmissionRow,
        r#"SELECT id AS "id: SubmissionId", workspace_id AS "workspace_id: WorkspaceId",
                  form_id AS "form_id: FormId", status, spam_reason, data, user_agent, referer,
                  created_at AS "created_at: UnixMillis"
           FROM form_submissions
           WHERE workspace_id = ?1 AND form_id = ?2 AND status = ?3 AND (?4 IS NULL OR id < ?4)
           ORDER BY id DESC LIMIT ?5"#,
        workspace_id,
        form_id,
        status,
        before,
        limit
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn find_submission(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    form_id: FormId,
    id: SubmissionId,
) -> sqlx::Result<Option<SubmissionRow>> {
    sqlx::query_as!(
        SubmissionRow,
        r#"SELECT id AS "id: SubmissionId", workspace_id AS "workspace_id: WorkspaceId",
                  form_id AS "form_id: FormId", status, spam_reason, data, user_agent, referer,
                  created_at AS "created_at: UnixMillis"
           FROM form_submissions WHERE workspace_id = ? AND form_id = ? AND id = ?"#,
        workspace_id,
        form_id,
        id
    )
    .fetch_optional(exec)
    .await
}

/// Moves a submission between inbox and spam. Marking it inbox clears the
/// spam reason.
pub(crate) async fn set_status(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    form_id: FormId,
    id: SubmissionId,
    status: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE form_submissions
         SET status = ?1, spam_reason = CASE WHEN ?1 = 'inbox' THEN NULL ELSE spam_reason END
         WHERE workspace_id = ?2 AND form_id = ?3 AND id = ?4",
        status,
        workspace_id,
        form_id,
        id
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn delete_submission(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    form_id: FormId,
    id: SubmissionId,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM form_submissions WHERE workspace_id = ? AND form_id = ? AND id = ?",
        workspace_id,
        form_id,
        id
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Submissions of a form with `status` since `since`, for the caps.
pub(crate) async fn recent(
    exec: impl SqliteExecutor<'_>,
    form: &FormRow,
    status: &str,
    since: UnixMillis,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM form_submissions
           WHERE form_id = ? AND workspace_id = ? AND status = ? AND created_at >= ?"#,
        form.id,
        form.workspace_id,
        status,
        since
    )
    .fetch_one(exec)
    .await
}

/// Records a solved challenge. False when it was already used. The
/// workspace's expired rows are pruned on the way.
pub(crate) async fn use_challenge(
    tx: &mut sqlx::SqliteConnection,
    form: &FormRow,
    challenge: &str,
    expires_at: UnixMillis,
    now: UnixMillis,
) -> sqlx::Result<bool> {
    sqlx::query!(
        "DELETE FROM form_used_challenges WHERE workspace_id = ? AND expires_at < ?",
        form.workspace_id,
        now
    )
    .execute(&mut *tx)
    .await?;
    let result = sqlx::query!(
        "INSERT INTO form_used_challenges (challenge, workspace_id, form_id, expires_at)
         VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
        challenge,
        form.workspace_id,
        form.id,
        expires_at
    )
    .execute(&mut *tx)
    .await?;
    Ok(result.rows_affected() == 1)
}
