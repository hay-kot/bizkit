//! The `sessions` table.

use sqlx::SqliteExecutor;

use crate::ids::UserId;
use crate::time::UnixMillis;

/// A live session and its user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionUser {
    pub(crate) user_id: UserId,
    pub(crate) email: String,
    pub(crate) last_seen_at: UnixMillis,
}

/// A new session's fields.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NewSession<'a> {
    pub(crate) token_hash: &'a str,
    pub(crate) user_id: UserId,
    pub(crate) expires_at: UnixMillis,
    pub(crate) user_agent: Option<&'a str>,
}

pub(crate) async fn insert(
    exec: impl SqliteExecutor<'_>,
    session: NewSession<'_>,
    now: UnixMillis,
) -> sqlx::Result<()> {
    let NewSession { token_hash, user_id, expires_at, user_agent } = session;
    sqlx::query!(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, last_seen_at, user_agent)
         VALUES (?, ?, ?, ?, ?, ?)",
        token_hash,
        user_id,
        now,
        expires_at,
        now,
        user_agent
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// The unexpired session with `token_hash`.
pub(crate) async fn find(
    exec: impl SqliteExecutor<'_>,
    token_hash: &str,
    now: UnixMillis,
) -> sqlx::Result<Option<SessionUser>> {
    sqlx::query_as!(
        SessionUser,
        r#"SELECT s.user_id AS "user_id: UserId", u.email, s.last_seen_at AS "last_seen_at: UnixMillis"
           FROM sessions s JOIN users u ON u.id = s.user_id
           WHERE s.token_hash = ? AND s.expires_at > ?"#,
        token_hash,
        now
    )
    .fetch_optional(exec)
    .await
}

/// Records use and slides the expiry forward.
pub(crate) async fn touch(
    exec: impl SqliteExecutor<'_>,
    token_hash: &str,
    now: UnixMillis,
    expires_at: UnixMillis,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE token_hash = ?",
        now,
        expires_at,
        token_hash
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn delete(exec: impl SqliteExecutor<'_>, token_hash: &str) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM sessions WHERE token_hash = ?", token_hash).execute(exec).await?;
    Ok(())
}
