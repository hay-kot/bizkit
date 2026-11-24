//! The mailing tables. Every query filters on `workspace_id`, except the
//! token lookups for the public pages: a link carries only its token, and
//! later queries use the workspace of the row it returns.

use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::time::UnixMillis;
use sqlx::SqliteExecutor;

use crate::api::{BroadcastId, ContactId, DeliveryId, EnrollmentId, ListId, StepId};

/// A `mailing_lists` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ListRow {
    pub(crate) id: ListId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) double_opt_in: bool,
    pub(crate) footer: Option<String>,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

/// A `mailing_contacts` row. `tags` stays JSON text and `status` stored text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContactRow {
    pub(crate) id: ContactId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) list_id: ListId,
    pub(crate) email: String,
    pub(crate) name: Option<String>,
    pub(crate) tags: String,
    pub(crate) status: String,
    pub(crate) source: String,
    pub(crate) confirm_token: Option<String>,
    pub(crate) confirm_sent_at: Option<UnixMillis>,
    pub(crate) unsubscribe_token: String,
    pub(crate) subscribed_at: Option<UnixMillis>,
    pub(crate) unsubscribed_at: Option<UnixMillis>,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

/// A `mailing_broadcasts` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BroadcastRow {
    pub(crate) id: BroadcastId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) list_id: ListId,
    pub(crate) subject: String,
    pub(crate) body_md: String,
    pub(crate) tag: Option<String>,
    pub(crate) status: String,
    pub(crate) recipients: i64,
    pub(crate) sent_at: Option<UnixMillis>,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

/// A `mailing_deliveries` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliveryRow {
    pub(crate) id: DeliveryId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) list_id: ListId,
    pub(crate) contact_id: Option<ContactId>,
    pub(crate) kind: String,
    pub(crate) broadcast_id: Option<BroadcastId>,
    pub(crate) enrollment_id: Option<EnrollmentId>,
    pub(crate) step_id: Option<StepId>,
    pub(crate) to_email: String,
    pub(crate) subject: String,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(crate) attempts: i64,
    pub(crate) created_at: UnixMillis,
    pub(crate) sent_at: Option<UnixMillis>,
}

/// Contacts of a list by stored status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) pending: i64,
    pub(crate) subscribed: i64,
    pub(crate) unsubscribed: i64,
}

/// The filters of the contact list.
#[derive(Debug, Clone, Default)]
pub(crate) struct ContactFilter<'a> {
    pub(crate) status: Option<&'a str>,
    pub(crate) tag: Option<&'a str>,
    pub(crate) q: Option<&'a str>,
    pub(crate) before: Option<ContactId>,
}

/// The filters of the delivery list. Test sends are always left out.
#[derive(Debug, Clone, Default)]
pub(crate) struct DeliveryFilter<'a> {
    pub(crate) broadcast_id: Option<BroadcastId>,
    pub(crate) contact_id: Option<ContactId>,
    pub(crate) status: Option<&'a str>,
    pub(crate) before: Option<DeliveryId>,
}

fn json_text<T: serde::Serialize>(value: &T) -> sqlx::Result<String> {
    serde_json::to_string(value).map_err(|err| sqlx::Error::Encode(Box::new(err)))
}

// ---------------------------------------------------------------- lists

pub(crate) async fn insert_list(exec: impl SqliteExecutor<'_>, row: &ListRow) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO mailing_lists (id, workspace_id, project_id, name, description,
                                    double_opt_in, footer, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.project_id,
        row.name,
        row.description,
        row.double_opt_in,
        row.footer,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Writes every settings column of `row`, matched by workspace and ID.
pub(crate) async fn update_list(exec: impl SqliteExecutor<'_>, row: &ListRow) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE mailing_lists SET name = ?, description = ?, double_opt_in = ?, footer = ?,
                                  updated_at = ?
         WHERE workspace_id = ? AND id = ?",
        row.name,
        row.description,
        row.double_opt_in,
        row.footer,
        row.updated_at,
        row.workspace_id,
        row.id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn find_list(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ListId,
) -> sqlx::Result<Option<ListRow>> {
    sqlx::query_as!(
        ListRow,
        r#"SELECT id AS "id: ListId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, description,
                  double_opt_in AS "double_opt_in: bool", footer,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM mailing_lists WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        id,
    )
    .fetch_optional(exec)
    .await
}

/// Lists of the workspace by name, only `project_id`'s when given.
pub(crate) async fn list_lists(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: Option<ProjectId>,
) -> sqlx::Result<Vec<ListRow>> {
    sqlx::query_as!(
        ListRow,
        r#"SELECT id AS "id: ListId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, description,
                  double_opt_in AS "double_opt_in: bool", footer,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM mailing_lists WHERE workspace_id = ?1 AND (?2 IS NULL OR project_id = ?2)
           ORDER BY name, id"#,
        workspace_id,
        project_id,
    )
    .fetch_all(exec)
    .await
}

/// The project's lists that `form_id` feeds.
pub(crate) async fn lists_for_form(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    form_id: &str,
) -> sqlx::Result<Vec<ListRow>> {
    sqlx::query_as!(
        ListRow,
        r#"SELECT l.id AS "id: ListId", l.workspace_id AS "workspace_id: WorkspaceId",
                  l.project_id AS "project_id: ProjectId", l.name, l.description,
                  l.double_opt_in AS "double_opt_in: bool", l.footer,
                  l.created_at AS "created_at: UnixMillis", l.updated_at AS "updated_at: UnixMillis"
           FROM mailing_lists l
           JOIN mailing_list_forms f ON f.list_id = l.id AND f.workspace_id = l.workspace_id
           WHERE l.workspace_id = ? AND l.project_id = ? AND f.form_id = ?
           ORDER BY l.name, l.id"#,
        workspace_id,
        project_id,
        form_id,
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn delete_list(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ListId,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM mailing_lists WHERE workspace_id = ? AND id = ?",
        workspace_id,
        id
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// `(list, form)` links of the workspace's lists among `list_ids`.
pub(crate) async fn list_forms(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    list_ids: &[ListId],
) -> sqlx::Result<Vec<(ListId, String)>> {
    let ids = json_text(&list_ids)?;
    let rows = sqlx::query!(
        r#"SELECT list_id AS "list_id: ListId", form_id FROM mailing_list_forms
           WHERE workspace_id = ? AND list_id IN (SELECT value FROM json_each(?))
           ORDER BY list_id, form_id"#,
        workspace_id,
        ids,
    )
    .fetch_all(exec)
    .await?;
    Ok(rows.into_iter().map(|r| (r.list_id, r.form_id)).collect())
}

/// Replaces the list's form links with `form_ids`.
pub(crate) async fn set_list_forms(
    exec: &mut sqlx::SqliteConnection,
    workspace_id: WorkspaceId,
    list_id: ListId,
    form_ids: &[String],
) -> sqlx::Result<()> {
    sqlx::query!(
        "DELETE FROM mailing_list_forms WHERE workspace_id = ? AND list_id = ?",
        workspace_id,
        list_id
    )
    .execute(&mut *exec)
    .await?;
    for form_id in form_ids {
        sqlx::query!(
            "INSERT OR IGNORE INTO mailing_list_forms (list_id, form_id, workspace_id)
             VALUES (?, ?, ?)",
            list_id,
            form_id,
            workspace_id,
        )
        .execute(&mut *exec)
        .await?;
    }
    Ok(())
}

pub(crate) async fn contact_counts(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    list_id: ListId,
) -> sqlx::Result<Counts> {
    let row = sqlx::query!(
        r#"SELECT COALESCE(SUM(status = 'pending'), 0) AS "pending!: i64",
                  COALESCE(SUM(status = 'subscribed'), 0) AS "subscribed!: i64",
                  COALESCE(SUM(status = 'unsubscribed'), 0) AS "unsubscribed!: i64"
           FROM mailing_contacts WHERE workspace_id = ? AND list_id = ?"#,
        workspace_id,
        list_id,
    )
    .fetch_one(exec)
    .await?;
    Ok(Counts { pending: row.pending, subscribed: row.subscribed, unsubscribed: row.unsubscribed })
}

// ---------------------------------------------------------------- contacts

pub(crate) async fn insert_contact(
    exec: impl SqliteExecutor<'_>,
    row: &ContactRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO mailing_contacts (id, workspace_id, list_id, email, name, tags, status, source,
                                       confirm_token, confirm_sent_at, unsubscribe_token,
                                       subscribed_at, unsubscribed_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.list_id,
        row.email,
        row.name,
        row.tags,
        row.status,
        row.source,
        row.confirm_token,
        row.confirm_sent_at,
        row.unsubscribe_token,
        row.subscribed_at,
        row.unsubscribed_at,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Writes every changeable column of `row`, matched by workspace and ID.
pub(crate) async fn update_contact(
    exec: impl SqliteExecutor<'_>,
    row: &ContactRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE mailing_contacts SET name = ?, tags = ?, status = ?, confirm_token = ?,
                                     confirm_sent_at = ?, subscribed_at = ?, unsubscribed_at = ?,
                                     updated_at = ?
         WHERE workspace_id = ? AND id = ?",
        row.name,
        row.tags,
        row.status,
        row.confirm_token,
        row.confirm_sent_at,
        row.subscribed_at,
        row.unsubscribed_at,
        row.updated_at,
        row.workspace_id,
        row.id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn find_contact(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ContactId,
) -> sqlx::Result<Option<ContactRow>> {
    sqlx::query_as!(
        ContactRow,
        r#"SELECT id AS "id: ContactId", workspace_id AS "workspace_id: WorkspaceId",
                  list_id AS "list_id: ListId", email, name, tags, status, source, confirm_token,
                  confirm_sent_at AS "confirm_sent_at: UnixMillis", unsubscribe_token,
                  subscribed_at AS "subscribed_at: UnixMillis",
                  unsubscribed_at AS "unsubscribed_at: UnixMillis",
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM mailing_contacts WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        id,
    )
    .fetch_optional(exec)
    .await
}

/// The contact for a lower-case address on a list.
pub(crate) async fn find_contact_by_email(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    list_id: ListId,
    email: &str,
) -> sqlx::Result<Option<ContactRow>> {
    sqlx::query_as!(
        ContactRow,
        r#"SELECT id AS "id: ContactId", workspace_id AS "workspace_id: WorkspaceId",
                  list_id AS "list_id: ListId", email, name, tags, status, source, confirm_token,
                  confirm_sent_at AS "confirm_sent_at: UnixMillis", unsubscribe_token,
                  subscribed_at AS "subscribed_at: UnixMillis",
                  unsubscribed_at AS "unsubscribed_at: UnixMillis",
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM mailing_contacts WHERE workspace_id = ? AND list_id = ? AND email = ?"#,
        workspace_id,
        list_id,
        email,
    )
    .fetch_optional(exec)
    .await
}

/// The contact a confirm link names, in any workspace.
pub(crate) async fn find_by_confirm_token(
    exec: impl SqliteExecutor<'_>,
    token: &str,
) -> sqlx::Result<Option<ContactRow>> {
    sqlx::query_as!(
        ContactRow,
        r#"SELECT id AS "id: ContactId", workspace_id AS "workspace_id: WorkspaceId",
                  list_id AS "list_id: ListId", email, name, tags, status, source, confirm_token,
                  confirm_sent_at AS "confirm_sent_at: UnixMillis", unsubscribe_token,
                  subscribed_at AS "subscribed_at: UnixMillis",
                  unsubscribed_at AS "unsubscribed_at: UnixMillis",
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM mailing_contacts WHERE confirm_token = ?"#,
        token,
    )
    .fetch_optional(exec)
    .await
}

/// The contact an unsubscribe link names, in any workspace.
pub(crate) async fn find_by_unsubscribe_token(
    exec: impl SqliteExecutor<'_>,
    token: &str,
) -> sqlx::Result<Option<ContactRow>> {
    sqlx::query_as!(
        ContactRow,
        r#"SELECT id AS "id: ContactId", workspace_id AS "workspace_id: WorkspaceId",
                  list_id AS "list_id: ListId", email, name, tags, status, source, confirm_token,
                  confirm_sent_at AS "confirm_sent_at: UnixMillis", unsubscribe_token,
                  subscribed_at AS "subscribed_at: UnixMillis",
                  unsubscribed_at AS "unsubscribed_at: UnixMillis",
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM mailing_contacts WHERE unsubscribe_token = ?"#,
        token,
    )
    .fetch_optional(exec)
    .await
}

/// Contacts of a list newest first, matching `filter`.
pub(crate) async fn list_contacts(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    list_id: ListId,
    filter: &ContactFilter<'_>,
    limit: i64,
) -> sqlx::Result<Vec<ContactRow>> {
    let q = filter.q.map(str::to_lowercase);
    sqlx::query_as!(
        ContactRow,
        r#"SELECT id AS "id: ContactId", workspace_id AS "workspace_id: WorkspaceId",
                  list_id AS "list_id: ListId", email, name, tags, status, source, confirm_token,
                  confirm_sent_at AS "confirm_sent_at: UnixMillis", unsubscribe_token,
                  subscribed_at AS "subscribed_at: UnixMillis",
                  unsubscribed_at AS "unsubscribed_at: UnixMillis",
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM mailing_contacts c
           WHERE workspace_id = ?1 AND list_id = ?2
             AND (?3 IS NULL OR status = ?3)
             AND (?4 IS NULL OR EXISTS (SELECT 1 FROM json_each(c.tags) t WHERE t.value = ?4))
             AND (?5 IS NULL OR instr(email, ?5) > 0 OR instr(lower(COALESCE(name, '')), ?5) > 0)
             AND (?6 IS NULL OR id < ?6)
           ORDER BY id DESC LIMIT ?7"#,
        workspace_id,
        list_id,
        filter.status,
        filter.tag,
        q,
        filter.before,
        limit,
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn delete_contact(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ContactId,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM mailing_contacts WHERE workspace_id = ? AND id = ?",
        workspace_id,
        id
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Up to `limit` of the broadcast's recipients after `after` in ID order:
/// its list's subscribed contacts, only those with its tag when it has one.
pub(crate) async fn subscribed_after(
    exec: impl SqliteExecutor<'_>,
    broadcast: &BroadcastRow,
    after: Option<ContactId>,
    limit: i64,
) -> sqlx::Result<Vec<(ContactId, String)>> {
    let rows = sqlx::query!(
        r#"SELECT id AS "id: ContactId", email FROM mailing_contacts c
           WHERE workspace_id = ?1 AND list_id = ?2 AND status = 'subscribed'
             AND (?3 IS NULL OR EXISTS (SELECT 1 FROM json_each(c.tags) t WHERE t.value = ?3))
             AND (?4 IS NULL OR id > ?4)
           ORDER BY id LIMIT ?5"#,
        broadcast.workspace_id,
        broadcast.list_id,
        broadcast.tag,
        after,
        limit,
    )
    .fetch_all(exec)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.email)).collect())
}

// ---------------------------------------------------------------- broadcasts

pub(crate) async fn insert_broadcast(
    exec: impl SqliteExecutor<'_>,
    row: &BroadcastRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO mailing_broadcasts (id, workspace_id, list_id, subject, body_md, tag, status,
                                         recipients, sent_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.list_id,
        row.subject,
        row.body_md,
        row.tag,
        row.status,
        row.recipients,
        row.sent_at,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Writes every changeable column of `row`, matched by workspace and ID.
pub(crate) async fn update_broadcast(
    exec: impl SqliteExecutor<'_>,
    row: &BroadcastRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE mailing_broadcasts SET list_id = ?, subject = ?, body_md = ?, tag = ?, status = ?,
                                       recipients = ?, sent_at = ?, updated_at = ?
         WHERE workspace_id = ? AND id = ?",
        row.list_id,
        row.subject,
        row.body_md,
        row.tag,
        row.status,
        row.recipients,
        row.sent_at,
        row.updated_at,
        row.workspace_id,
        row.id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn find_broadcast(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: BroadcastId,
) -> sqlx::Result<Option<BroadcastRow>> {
    sqlx::query_as!(
        BroadcastRow,
        r#"SELECT id AS "id: BroadcastId", workspace_id AS "workspace_id: WorkspaceId",
                  list_id AS "list_id: ListId", subject, body_md, tag, status, recipients,
                  sent_at AS "sent_at: UnixMillis", created_at AS "created_at: UnixMillis",
                  updated_at AS "updated_at: UnixMillis"
           FROM mailing_broadcasts WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        id,
    )
    .fetch_optional(exec)
    .await
}

/// Broadcasts newest first, only those of `project_id`'s lists or of
/// `list_id` when given.
pub(crate) async fn list_broadcasts(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: Option<ProjectId>,
    list_id: Option<ListId>,
) -> sqlx::Result<Vec<BroadcastRow>> {
    sqlx::query_as!(
        BroadcastRow,
        r#"SELECT b.id AS "id: BroadcastId", b.workspace_id AS "workspace_id: WorkspaceId",
                  b.list_id AS "list_id: ListId", b.subject, b.body_md, b.tag, b.status,
                  b.recipients, b.sent_at AS "sent_at: UnixMillis",
                  b.created_at AS "created_at: UnixMillis", b.updated_at AS "updated_at: UnixMillis"
           FROM mailing_broadcasts b
           JOIN mailing_lists l ON l.id = b.list_id AND l.workspace_id = b.workspace_id
           WHERE b.workspace_id = ?1 AND (?2 IS NULL OR l.project_id = ?2)
             AND (?3 IS NULL OR b.list_id = ?3)
           ORDER BY b.id DESC"#,
        workspace_id,
        project_id,
        list_id,
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn delete_broadcast(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: BroadcastId,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM mailing_broadcasts WHERE workspace_id = ? AND id = ?",
        workspace_id,
        id
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

// ---------------------------------------------------------------- deliveries

/// Inserts `row` unless the broadcast or the step already has a delivery for
/// the contact or enrollment. True when it was inserted.
pub(crate) async fn insert_delivery(
    exec: impl SqliteExecutor<'_>,
    row: &DeliveryRow,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "INSERT OR IGNORE INTO mailing_deliveries
           (id, workspace_id, list_id, contact_id, kind, broadcast_id, enrollment_id, step_id,
            to_email, subject, status, error, attempts, created_at, sent_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.list_id,
        row.contact_id,
        row.kind,
        row.broadcast_id,
        row.enrollment_id,
        row.step_id,
        row.to_email,
        row.subject,
        row.status,
        row.error,
        row.attempts,
        row.created_at,
        row.sent_at,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn find_delivery(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: DeliveryId,
) -> sqlx::Result<Option<DeliveryRow>> {
    sqlx::query_as!(
        DeliveryRow,
        r#"SELECT id AS "id: DeliveryId", workspace_id AS "workspace_id: WorkspaceId",
                  list_id AS "list_id: ListId", contact_id AS "contact_id: ContactId", kind,
                  broadcast_id AS "broadcast_id: BroadcastId",
                  enrollment_id AS "enrollment_id: EnrollmentId", step_id AS "step_id: StepId",
                  to_email, subject, status, error, attempts,
                  created_at AS "created_at: UnixMillis", sent_at AS "sent_at: UnixMillis"
           FROM mailing_deliveries WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        id,
    )
    .fetch_optional(exec)
    .await
}

/// Records the outcome of a send attempt.
pub(crate) async fn mark_delivery(
    exec: impl SqliteExecutor<'_>,
    row: &DeliveryRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE mailing_deliveries SET status = ?, error = ?, attempts = ?, sent_at = ?
         WHERE workspace_id = ? AND id = ?",
        row.status,
        row.error,
        row.attempts,
        row.sent_at,
        row.workspace_id,
        row.id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Deliveries newest first, matching `filter`.
pub(crate) async fn list_deliveries(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    filter: &DeliveryFilter<'_>,
    limit: i64,
) -> sqlx::Result<Vec<DeliveryRow>> {
    sqlx::query_as!(
        DeliveryRow,
        r#"SELECT id AS "id: DeliveryId", workspace_id AS "workspace_id: WorkspaceId",
                  list_id AS "list_id: ListId", contact_id AS "contact_id: ContactId", kind,
                  broadcast_id AS "broadcast_id: BroadcastId",
                  enrollment_id AS "enrollment_id: EnrollmentId", step_id AS "step_id: StepId",
                  to_email, subject, status, error, attempts,
                  created_at AS "created_at: UnixMillis", sent_at AS "sent_at: UnixMillis"
           FROM mailing_deliveries
           WHERE workspace_id = ?1 AND kind != 'test'
             AND (?2 IS NULL OR broadcast_id = ?2)
             AND (?3 IS NULL OR contact_id = ?3)
             AND (?4 IS NULL OR status = ?4)
             AND (?5 IS NULL OR id < ?5)
           ORDER BY id DESC LIMIT ?6"#,
        workspace_id,
        filter.broadcast_id,
        filter.contact_id,
        filter.status,
        filter.before,
        limit,
    )
    .fetch_all(exec)
    .await
}

/// Confirm mails queued for a list since `since`.
pub(crate) async fn confirms_since(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    list_id: ListId,
    since: UnixMillis,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM mailing_deliveries
           WHERE workspace_id = ? AND list_id = ? AND kind = 'confirm' AND created_at >= ?"#,
        workspace_id,
        list_id,
        since,
    )
    .fetch_one(exec)
    .await
}

/// Test sends queued in the workspace since `since`.
pub(crate) async fn tests_since(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    since: UnixMillis,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM mailing_deliveries
           WHERE workspace_id = ? AND kind = 'test' AND created_at >= ?"#,
        workspace_id,
        since,
    )
    .fetch_one(exec)
    .await
}

/// The broadcast's highest contact ID with a delivery, where fan-out resumes.
pub(crate) async fn last_broadcast_contact(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    broadcast_id: BroadcastId,
) -> sqlx::Result<Option<ContactId>> {
    sqlx::query_scalar!(
        r#"SELECT MAX(contact_id) AS "contact_id: ContactId" FROM mailing_deliveries
           WHERE workspace_id = ? AND broadcast_id = ? AND kind = 'broadcast'"#,
        workspace_id,
        broadcast_id,
    )
    .fetch_one(exec)
    .await
}

/// Broadcast deliveries of the broadcast.
pub(crate) async fn broadcast_recipients(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    broadcast_id: BroadcastId,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM mailing_deliveries
           WHERE workspace_id = ? AND broadcast_id = ? AND kind = 'broadcast'"#,
        workspace_id,
        broadcast_id,
    )
    .fetch_one(exec)
    .await
}
