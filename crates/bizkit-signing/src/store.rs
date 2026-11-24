//! Queries for signature requests, signers, files and the audit trail.

use bizkit_core::ids::{ProjectId, UserId, WorkspaceId};
use bizkit_core::time::UnixMillis;
use sqlx::SqliteExecutor;

use crate::api::{DocumentVersion, EventKind, RequestStatus, SignerId, SigningRequestId};

/// A `signing_requests` row.
#[derive(Debug, Clone)]
pub(crate) struct RequestRow {
    pub(crate) id: SigningRequestId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) status: String,
    pub(crate) file_name: String,
    pub(crate) original_sha256: String,
    pub(crate) signed_sha256: Option<String>,
    pub(crate) created_by: Option<UserId>,
    pub(crate) sender: String,
    pub(crate) failure: Option<String>,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
    pub(crate) sent_at: Option<UnixMillis>,
    pub(crate) completed_at: Option<UnixMillis>,
    pub(crate) voided_at: Option<UnixMillis>,
}

/// A `signing_signers` row.
#[derive(Debug, Clone)]
pub(crate) struct SignerRow {
    pub(crate) id: SignerId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) request_id: SigningRequestId,
    pub(crate) position: i64,
    pub(crate) name: String,
    pub(crate) email: String,
    pub(crate) status: String,
    pub(crate) signature: Option<String>,
    pub(crate) signed_at: Option<UnixMillis>,
}

/// A `signing_events` row.
#[derive(Debug, Clone)]
pub(crate) struct EventRow {
    pub(crate) kind: String,
    pub(crate) at: UnixMillis,
    pub(crate) signer_id: Option<SignerId>,
    pub(crate) ip: Option<String>,
    pub(crate) user_agent: Option<String>,
    pub(crate) detail: Option<String>,
}

/// An event to add.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NewEvent<'a> {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) request_id: SigningRequestId,
    pub(crate) signer_id: Option<SignerId>,
    pub(crate) kind: EventKind,
    pub(crate) at: UnixMillis,
    pub(crate) ip: Option<&'a str>,
    pub(crate) user_agent: Option<&'a str>,
    pub(crate) detail: Option<&'a str>,
}

impl NewEvent<'_> {
    /// An event with no signer and no client details.
    pub(crate) const fn of(row: &RequestRow, kind: EventKind, at: UnixMillis) -> Self {
        Self {
            workspace_id: row.workspace_id,
            request_id: row.id,
            signer_id: None,
            kind,
            at,
            ip: None,
            user_agent: None,
            detail: None,
        }
    }
}

pub(crate) async fn insert_request(
    exec: impl SqliteExecutor<'_>,
    row: &RequestRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO signing_requests (id, workspace_id, project_id, title, message, status, file_name,
             original_sha256, signed_sha256, created_by, sender, failure, created_at, updated_at, sent_at,
             completed_at, voided_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.project_id,
        row.title,
        row.message,
        row.status,
        row.file_name,
        row.original_sha256,
        row.signed_sha256,
        row.created_by,
        row.sender,
        row.failure,
        row.created_at,
        row.updated_at,
        row.sent_at,
        row.completed_at,
        row.voided_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn find_request(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: SigningRequestId,
) -> sqlx::Result<Option<RequestRow>> {
    sqlx::query_as!(
        RequestRow,
        r#"SELECT id AS "id: SigningRequestId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", title, message, status, file_name,
                  original_sha256, signed_sha256, created_by AS "created_by: UserId", sender, failure,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis",
                  sent_at AS "sent_at: UnixMillis", completed_at AS "completed_at: UnixMillis",
                  voided_at AS "voided_at: UnixMillis"
           FROM signing_requests WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        id,
    )
    .fetch_optional(exec)
    .await
}

pub(crate) async fn list_requests(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<Vec<RequestRow>> {
    sqlx::query_as!(
        RequestRow,
        r#"SELECT id AS "id: SigningRequestId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", title, message, status, file_name,
                  original_sha256, signed_sha256, created_by AS "created_by: UserId", sender, failure,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis",
                  sent_at AS "sent_at: UnixMillis", completed_at AS "completed_at: UnixMillis",
                  voided_at AS "voided_at: UnixMillis"
           FROM signing_requests WHERE workspace_id = ? AND project_id = ?
           ORDER BY created_at DESC, id DESC LIMIT 200"#,
        workspace_id,
        project_id,
    )
    .fetch_all(exec)
    .await
}

/// Moves a request from one of `from` to `to`, stamping the matching time.
/// False when it wasn't in one of `from`.
pub(crate) async fn set_status(
    exec: impl SqliteExecutor<'_>,
    row: &RequestRow,
    from: &[RequestStatus],
    to: RequestStatus,
    now: UnixMillis,
) -> sqlx::Result<bool> {
    let allowed = from.iter().any(|status| status.as_str() == row.status);
    if !allowed {
        return Ok(false);
    }
    let to_text = to.as_str();
    let sent = (to == RequestStatus::Sent).then_some(now);
    let completed = (to == RequestStatus::Completed).then_some(now);
    let voided = (to == RequestStatus::Voided).then_some(now);
    let result = sqlx::query!(
        "UPDATE signing_requests
         SET status = ?, updated_at = ?, failure = NULL, sent_at = coalesce(?, sent_at),
             completed_at = coalesce(?, completed_at), voided_at = coalesce(?, voided_at)
         WHERE workspace_id = ? AND id = ? AND status = ?",
        to_text,
        now,
        sent,
        completed,
        voided,
        row.workspace_id,
        row.id,
        row.status,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn set_signed_sha256(
    exec: impl SqliteExecutor<'_>,
    row: &RequestRow,
    sha256: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE signing_requests SET signed_sha256 = ? WHERE workspace_id = ? AND id = ?",
        sha256,
        row.workspace_id,
        row.id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn delete_request(
    exec: impl SqliteExecutor<'_>,
    row: &RequestRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "DELETE FROM signing_requests WHERE workspace_id = ? AND id = ?",
        row.workspace_id,
        row.id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn put_file(
    exec: impl SqliteExecutor<'_>,
    row: &RequestRow,
    version: DocumentVersion,
    bytes: &[u8],
) -> sqlx::Result<()> {
    let kind = version.as_str();
    sqlx::query!(
        "INSERT INTO signing_files (request_id, workspace_id, kind, bytes) VALUES (?, ?, ?, ?)
         ON CONFLICT (request_id, kind) DO UPDATE SET bytes = excluded.bytes",
        row.id,
        row.workspace_id,
        kind,
        bytes,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn get_file(
    exec: impl SqliteExecutor<'_>,
    row: &RequestRow,
    version: DocumentVersion,
) -> sqlx::Result<Option<Vec<u8>>> {
    let kind = version.as_str();
    sqlx::query_scalar!(
        "SELECT bytes FROM signing_files WHERE workspace_id = ? AND request_id = ? AND kind = ?",
        row.workspace_id,
        row.id,
        kind,
    )
    .fetch_optional(exec)
    .await
}

pub(crate) async fn insert_signer(
    exec: impl SqliteExecutor<'_>,
    row: &SignerRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO signing_signers (id, workspace_id, request_id, position, name, email, status)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.request_id,
        row.position,
        row.name,
        row.email,
        row.status,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn signers(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    request_id: SigningRequestId,
) -> sqlx::Result<Vec<SignerRow>> {
    sqlx::query_as!(
        SignerRow,
        r#"SELECT id AS "id: SignerId", workspace_id AS "workspace_id: WorkspaceId",
                  request_id AS "request_id: SigningRequestId", position, name, email, status,
                  signature, signed_at AS "signed_at: UnixMillis"
           FROM signing_signers WHERE workspace_id = ? AND request_id = ? ORDER BY position"#,
        workspace_id,
        request_id,
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn set_token(
    exec: impl SqliteExecutor<'_>,
    signer: &SignerRow,
    token_hash: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE signing_signers SET token_hash = ? WHERE workspace_id = ? AND id = ?",
        token_hash,
        signer.workspace_id,
        signer.id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn insert_event(
    exec: impl SqliteExecutor<'_>,
    event: NewEvent<'_>,
) -> sqlx::Result<()> {
    let kind = event.kind.as_str();
    sqlx::query!(
        "INSERT INTO signing_events (workspace_id, request_id, signer_id, kind, at, ip, user_agent, detail)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        event.workspace_id,
        event.request_id,
        event.signer_id,
        kind,
        event.at,
        event.ip,
        event.user_agent,
        event.detail,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn events(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    request_id: SigningRequestId,
) -> sqlx::Result<Vec<EventRow>> {
    sqlx::query_as!(
        EventRow,
        r#"SELECT kind, at AS "at: UnixMillis", signer_id AS "signer_id: SignerId", ip, user_agent, detail
           FROM signing_events WHERE workspace_id = ? AND request_id = ? ORDER BY id"#,
        workspace_id,
        request_id,
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn set_failed(
    exec: impl SqliteExecutor<'_>,
    row: &RequestRow,
    failure: &str,
    now: UnixMillis,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE signing_requests SET status = 'failed', failure = ?, updated_at = ?
         WHERE workspace_id = ? AND id = ? AND status = 'sent'",
        failure,
        now,
        row.workspace_id,
        row.id,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// The signer whose link token hashes to `token_hash`: an unguessable key,
/// so this is the one lookup without a workspace.
pub(crate) async fn signer_by_token(
    exec: impl SqliteExecutor<'_>,
    token_hash: &str,
) -> sqlx::Result<Option<SignerRow>> {
    sqlx::query_as!(
        SignerRow,
        r#"SELECT id AS "id: SignerId", workspace_id AS "workspace_id: WorkspaceId",
                  request_id AS "request_id: SigningRequestId", position, name, email, status,
                  signature, signed_at AS "signed_at: UnixMillis"
           FROM signing_signers WHERE token_hash = ?"#,
        token_hash,
    )
    .fetch_optional(exec)
    .await
}

pub(crate) async fn has_viewed(
    exec: impl SqliteExecutor<'_>,
    signer: &SignerRow,
) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM signing_events
                         WHERE workspace_id = ? AND signer_id = ? AND kind = 'viewed') AS "seen!: bool""#,
        signer.workspace_id,
        signer.id,
    )
    .fetch_one(exec)
    .await
}

/// Marks a pending signer signed. False when they already were.
pub(crate) async fn mark_signed(
    exec: impl SqliteExecutor<'_>,
    signer: &SignerRow,
    signature: &str,
    now: UnixMillis,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE signing_signers SET status = 'signed', signature = ?, signed_at = ?
         WHERE workspace_id = ? AND id = ? AND status = 'pending'",
        signature,
        now,
        signer.workspace_id,
        signer.id,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn pending_signers(
    exec: impl SqliteExecutor<'_>,
    row: &RequestRow,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM signing_signers
           WHERE workspace_id = ? AND request_id = ? AND status = 'pending'"#,
        row.workspace_id,
        row.id,
    )
    .fetch_one(exec)
    .await
}

/// Signer mails a workspace queued since `since`, for the hourly cap.
pub(crate) async fn mails_since(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    since: UnixMillis,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM signing_events
           WHERE workspace_id = ? AND kind = 'sent' AND signer_id IS NOT NULL AND at >= ?"#,
        workspace_id,
        since,
    )
    .fetch_one(exec)
    .await
}

/// The instance's seal: certificate DER and private scalar.
pub(crate) async fn seal(
    exec: impl SqliteExecutor<'_>,
) -> sqlx::Result<Option<(Vec<u8>, Vec<u8>)>> {
    let row = sqlx::query!("SELECT certificate, private_key FROM signing_seal WHERE id = 1")
        .fetch_optional(exec)
        .await?;
    Ok(row.map(|row| (row.certificate, row.private_key)))
}

/// Stores the seal unless another worker stored one first.
pub(crate) async fn insert_seal(
    exec: impl SqliteExecutor<'_>,
    certificate: &[u8],
    private_key: &[u8],
    now: UnixMillis,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT OR IGNORE INTO signing_seal (id, certificate, private_key, created_at) VALUES (1, ?, ?, ?)",
        certificate,
        private_key,
        now,
    )
    .execute(exec)
    .await?;
    Ok(())
}
