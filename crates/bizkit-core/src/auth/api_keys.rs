//! The `api_keys` table.

use sqlx::SqliteExecutor;

use crate::ids::{ApiKeyId, UserId, WorkspaceId};
use crate::time::UnixMillis;

/// An API key row. `scopes` is the stored JSON array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApiKeyRow {
    pub(crate) id: ApiKeyId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) name: String,
    pub(crate) prefix: String,
    pub(crate) scopes: String,
    pub(crate) created_by: Option<UserId>,
    pub(crate) created_at: UnixMillis,
    pub(crate) expires_at: Option<UnixMillis>,
    pub(crate) last_used_at: Option<UnixMillis>,
    pub(crate) revoked_at: Option<UnixMillis>,
}

/// A new key's fields.
#[derive(Debug, Clone)]
pub(crate) struct NewApiKey<'a> {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) name: &'a str,
    pub(crate) prefix: &'a str,
    pub(crate) secret_hash: &'a str,
    pub(crate) scopes: &'a str,
    pub(crate) created_by: Option<UserId>,
    pub(crate) expires_at: Option<UnixMillis>,
}

pub(crate) async fn insert(
    exec: impl SqliteExecutor<'_>,
    key: &NewApiKey<'_>,
    now: UnixMillis,
) -> sqlx::Result<ApiKeyRow> {
    let id = ApiKeyId::new();
    sqlx::query!(
        "INSERT INTO api_keys (id, workspace_id, name, prefix, secret_hash, scopes, created_by, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        id,
        key.workspace_id,
        key.name,
        key.prefix,
        key.secret_hash,
        key.scopes,
        key.created_by,
        now,
        key.expires_at
    )
    .execute(exec)
    .await?;
    Ok(ApiKeyRow {
        id,
        workspace_id: key.workspace_id,
        name: key.name.to_owned(),
        prefix: key.prefix.to_owned(),
        scopes: key.scopes.to_owned(),
        created_by: key.created_by,
        created_at: now,
        expires_at: key.expires_at,
        last_used_at: None,
        revoked_at: None,
    })
}

/// The unrevoked, unexpired key with `secret_hash`.
pub(crate) async fn find_active(
    exec: impl SqliteExecutor<'_>,
    secret_hash: &str,
    now: UnixMillis,
) -> sqlx::Result<Option<ApiKeyRow>> {
    sqlx::query_as!(
        ApiKeyRow,
        r#"SELECT id AS "id: ApiKeyId", workspace_id AS "workspace_id: WorkspaceId", name, prefix, scopes,
                  created_by AS "created_by: UserId", created_at AS "created_at: UnixMillis",
                  expires_at AS "expires_at: UnixMillis", last_used_at AS "last_used_at: UnixMillis",
                  revoked_at AS "revoked_at: UnixMillis"
           FROM api_keys
           WHERE secret_hash = ? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)"#,
        secret_hash,
        now
    )
    .fetch_optional(exec)
    .await
}

pub(crate) async fn list(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
) -> sqlx::Result<Vec<ApiKeyRow>> {
    sqlx::query_as!(
        ApiKeyRow,
        r#"SELECT id AS "id: ApiKeyId", workspace_id AS "workspace_id: WorkspaceId", name, prefix, scopes,
                  created_by AS "created_by: UserId", created_at AS "created_at: UnixMillis",
                  expires_at AS "expires_at: UnixMillis", last_used_at AS "last_used_at: UnixMillis",
                  revoked_at AS "revoked_at: UnixMillis"
           FROM api_keys WHERE workspace_id = ? ORDER BY created_at DESC, id"#,
        workspace_id
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn touch(
    exec: impl SqliteExecutor<'_>,
    id: ApiKeyId,
    now: UnixMillis,
) -> sqlx::Result<()> {
    sqlx::query!("UPDATE api_keys SET last_used_at = ? WHERE id = ?", now, id)
        .execute(exec)
        .await?;
    Ok(())
}

/// Revokes an unrevoked key in `workspace_id`. False when there is none.
pub(crate) async fn revoke(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: ApiKeyId,
    now: UnixMillis,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE api_keys SET revoked_at = ? WHERE id = ? AND workspace_id = ? AND revoked_at IS NULL",
        now,
        id,
        workspace_id
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}
