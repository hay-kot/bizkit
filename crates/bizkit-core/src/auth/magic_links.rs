//! The `magic_links` table.

use sqlx::SqliteExecutor;

use crate::time::UnixMillis;

pub(crate) async fn insert(
    exec: impl SqliteExecutor<'_>,
    token_hash: &str,
    email: &str,
    now: UnixMillis,
    expires_at: UnixMillis,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO magic_links (token_hash, email, created_at, expires_at) VALUES (?, ?, ?, ?)",
        token_hash,
        email,
        now,
        expires_at
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// When the newest link for `email` went out.
pub(crate) async fn latest_sent_at(
    exec: impl SqliteExecutor<'_>,
    email: &str,
) -> sqlx::Result<Option<UnixMillis>> {
    let row = sqlx::query_scalar!(
        r#"SELECT MAX(created_at) AS "at: UnixMillis" FROM magic_links WHERE email = ?"#,
        email
    )
    .fetch_one(exec)
    .await?;
    Ok(row)
}

/// Marks an unused, unexpired link consumed and returns its email.
pub(crate) async fn consume(
    exec: impl SqliteExecutor<'_>,
    token_hash: &str,
    now: UnixMillis,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar!(
        "UPDATE magic_links SET consumed_at = ?
         WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?
         RETURNING email",
        now,
        token_hash,
        now
    )
    .fetch_optional(exec)
    .await
}
