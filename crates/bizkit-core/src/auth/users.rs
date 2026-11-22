//! The `users` table.

use sqlx::SqliteExecutor;

use crate::ids::UserId;
use crate::time::UnixMillis;

/// A user row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct User {
    pub(crate) id: UserId,
    pub(crate) email: String,
    pub(crate) name: Option<String>,
    pub(crate) created_at: UnixMillis,
    pub(crate) last_sign_in_at: Option<UnixMillis>,
}

pub(crate) async fn find_by_email(
    exec: impl SqliteExecutor<'_>,
    email: &str,
) -> sqlx::Result<Option<User>> {
    sqlx::query_as!(
        User,
        r#"SELECT id AS "id: UserId", email, name, created_at AS "created_at: UnixMillis",
                  last_sign_in_at AS "last_sign_in_at: UnixMillis"
           FROM users WHERE email = ?"#,
        email
    )
    .fetch_optional(exec)
    .await
}

pub(crate) async fn find(exec: impl SqliteExecutor<'_>, id: UserId) -> sqlx::Result<Option<User>> {
    sqlx::query_as!(
        User,
        r#"SELECT id AS "id: UserId", email, name, created_at AS "created_at: UnixMillis",
                  last_sign_in_at AS "last_sign_in_at: UnixMillis"
           FROM users WHERE id = ?"#,
        id
    )
    .fetch_optional(exec)
    .await
}

/// Inserts a user with a normalized `email`.
pub(crate) async fn insert(
    exec: impl SqliteExecutor<'_>,
    email: &str,
    now: UnixMillis,
) -> sqlx::Result<User> {
    let id = UserId::new();
    sqlx::query!("INSERT INTO users (id, email, created_at) VALUES (?, ?, ?)", id, email, now)
        .execute(exec)
        .await?;
    Ok(User { id, email: email.to_owned(), name: None, created_at: now, last_sign_in_at: None })
}

pub(crate) async fn touch_sign_in(
    exec: impl SqliteExecutor<'_>,
    id: UserId,
    now: UnixMillis,
) -> sqlx::Result<()> {
    sqlx::query!("UPDATE users SET last_sign_in_at = ? WHERE id = ?", now, id)
        .execute(exec)
        .await?;
    Ok(())
}
