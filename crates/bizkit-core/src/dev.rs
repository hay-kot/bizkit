//! Development and end-to-end helpers. Built only with the `test-hooks`
//! feature.

pub mod demo;
pub mod personas;

use anyhow::Context as _;
use url::Url;

use crate::auth::{hash_token, magic_links, normalize_email, random_token};
use crate::db::Db;
use crate::time::UnixMillis;

/// Writes a sign-in link for `email` straight into the database and returns
/// it. Verifying still applies the usual admission rules to new emails.
///
/// # Errors
///
/// When the database write fails.
pub async fn login_link(
    db: &Db,
    base_url: &Url,
    email: &str,
    now: UnixMillis,
) -> anyhow::Result<Url> {
    let email = normalize_email(email);
    let token = random_token();
    let mut tx = db.write().await?;
    magic_links::insert(
        &mut *tx,
        &hash_token(&token),
        &email,
        now,
        now.plus(std::time::Duration::from_mins(15)),
    )
    .await
    .context("inserting the link")?;
    tx.commit().await?;
    let mut link = base_url.join("/sign-in/verify").context("building the link")?;
    link.query_pairs_mut().append_pair("token", &token);
    Ok(link)
}
