//! The manage-subscriptions link: one signed, expiring link per contact
//! row that opens a page for the address on every list of the row's
//! project.
//!
//! The token is `<contact_id>.<expires_secs>.<mac>`, where the MAC is an
//! HMAC-SHA256 keyed by the workspace's link key (`mailing_link_keys`).
//! The key never leaves the database, unlike the unsubscribe token that
//! every mail prints, so nobody outside can mint a link or stretch its
//! expiry.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::auth::random_token;
use bizkit_core::db::WriteTx;
use bizkit_core::ids::WorkspaceId;
use bizkit_core::time::UnixMillis;
use hmac::{KeyInit as _, Mac as _};
use sqlx::SqliteConnection;
use url::Url;

use crate::api::{ContactId, ContactStatus, ListId};
use crate::contacts;
use crate::store::{self, AddressContactRow, ContactRow};

/// How long a manage link works after its mail was rendered. Every mail
/// carries a fresh one, and the per-list unsubscribe link never expires.
pub const MANAGE_TTL: Duration = Duration::from_hours(24 * 90);

/// Separates the MAC input from any other use of the key.
const DOMAIN: &[u8] = b"bizkit.mailing.manage";

type HmacSha256 = hmac::Hmac<sha2::Sha256>;

/// The workspace's link key, created in its own write transaction on first
/// use. Callers must not hold a write transaction.
pub(crate) async fn link_key(
    platform: &Platform,
    workspace_id: WorkspaceId,
) -> anyhow::Result<String> {
    let db = platform.db().read();
    if let Some(key) =
        store::find_link_key(db, workspace_id).await.context("loading the link key")?
    {
        return Ok(key);
    }
    let mut tx = platform.db().write().await?;
    store::insert_link_key(&mut *tx, workspace_id, &random_token(), platform.now())
        .await
        .context("creating the link key")?;
    let key = store::find_link_key(&mut *tx, workspace_id)
        .await
        .context("loading the link key")?
        .context("the link key is missing after its insert")?;
    tx.commit().await?;
    Ok(key)
}

/// Where `contact`'s manage link points, valid for [`MANAGE_TTL`].
pub(crate) fn manage_url(
    platform: &Platform,
    key: &str,
    contact: &ContactRow,
) -> anyhow::Result<Url> {
    let expires = platform.now().plus(MANAGE_TTL).0 / 1000;
    let mut url =
        platform.base_url().join("/m/mailing/public/manage").context("building a manage link")?;
    url.query_pairs_mut().append_pair("t", &token(key, contact.id, expires)?);
    Ok(url)
}

fn token(key: &str, id: ContactId, expires_secs: i64) -> anyhow::Result<String> {
    let mac = mac(key, id, expires_secs)?.finalize().into_bytes();
    Ok(format!("{id}.{expires_secs}.{}", hex::encode(mac)))
}

fn mac(key: &str, id: ContactId, expires_secs: i64) -> anyhow::Result<HmacSha256> {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).context("keying the manage link")?;
    mac.update(DOMAIN);
    mac.update(&[0]);
    mac.update(id.to_string().as_bytes());
    mac.update(&[0]);
    mac.update(expires_secs.to_string().as_bytes());
    Ok(mac)
}

/// A token's parts: contact ID, expiry in unix seconds and MAC.
fn parse(token: &str) -> Option<(ContactId, i64, Vec<u8>)> {
    let mut parts = token.split('.');
    let (Some(id), Some(expires), Some(sig), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    Some((id.parse().ok()?, expires.parse().ok()?, hex::decode(sig).ok()?))
}

/// Whether `sig` is the MAC of the link for `id` expiring at
/// `expires_secs`. The comparison takes constant time.
fn signed(key: &str, id: ContactId, expires_secs: i64, sig: &[u8]) -> bool {
    mac(key, id, expires_secs).is_ok_and(|mac| mac.verify_slice(sig).is_ok())
}

/// What a manage token names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verified {
    /// A valid link for this row.
    Contact(Box<ContactRow>),
    /// A valid link past its expiry.
    Expired,
    /// Malformed, forged, or for a row that no longer exists.
    Invalid,
}

/// Checks a manage token. The signature is checked before the expiry, so
/// "expired" is only ever said about a link bizkit made.
pub(crate) async fn verify(
    conn: &mut SqliteConnection,
    token: &str,
    now: UnixMillis,
) -> anyhow::Result<Verified> {
    let Some((id, expires, sig)) = parse(token) else {
        return Ok(Verified::Invalid);
    };
    let Some(contact) =
        store::find_contact_any(&mut *conn, id).await.context("looking up the contact")?
    else {
        return Ok(Verified::Invalid);
    };
    let Some(key) = store::find_link_key(&mut *conn, contact.workspace_id)
        .await
        .context("loading the link key")?
    else {
        return Ok(Verified::Invalid);
    };
    if !signed(&key, contact.id, expires, &sig) {
        return Ok(Verified::Invalid);
    }
    if expires.saturating_mul(1000) <= now.0 {
        return Ok(Verified::Expired);
    }
    Ok(Verified::Contact(Box::new(contact)))
}

/// The address's rows on the lists of `contact`'s project, by list name.
pub(crate) async fn rows(
    conn: &mut SqliteConnection,
    contact: &ContactRow,
) -> anyhow::Result<Vec<AddressContactRow>> {
    let Some(list) = store::find_list(&mut *conn, contact.workspace_id, contact.list_id)
        .await
        .context("loading the list")?
    else {
        return Ok(Vec::new());
    };
    store::address_contacts(conn, contact.workspace_id, &contact.email, Some(list.project_id))
        .await
        .context("loading the address's lists")
}

/// What the page's form asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change {
    /// Leave every list.
    All,
    /// Leave the lists the page showed and the reader unticked. Lists the
    /// page didn't show stay as they are.
    Keep { shown: HashSet<ListId>, keep: HashSet<ListId> },
}

/// Unsubscribes the address from the lists `change` leaves, among the
/// lists of `contact`'s project. Returns how many rows changed.
pub(crate) async fn apply(
    tx: &mut WriteTx,
    contact: &ContactRow,
    change: &Change,
    now: UnixMillis,
) -> anyhow::Result<usize> {
    let mut changed = 0;
    for row in rows(tx, contact).await? {
        let mut row = row.contact;
        let leaving = match change {
            Change::All => true,
            Change::Keep { shown, keep } => {
                shown.contains(&row.list_id) && !keep.contains(&row.list_id)
            }
        };
        if leaving && row.status != ContactStatus::Unsubscribed.as_str() {
            contacts::unsubscribe(tx, &mut row, now).await?;
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "workspace-key";

    fn check(token: &str, key: &str, id: ContactId) -> bool {
        parse(token).is_some_and(|(at, expires, sig)| at == id && signed(key, id, expires, &sig))
    }

    #[test]
    fn tokens_name_the_contact_and_expiry() {
        let id = ContactId::new();
        let token = token(KEY, id, 1_700_000_000).unwrap();
        assert!(token.starts_with(&format!("{id}.1700000000.")), "{token}");
        assert!(check(&token, KEY, id), "verifies");
    }

    #[test]
    fn tokens_break_when_any_part_changes() {
        let id = ContactId::new();
        let good = token(KEY, id, 1_700_000_000).unwrap();
        let later = good.replacen(".1700000000.", ".1800000000.", 1);
        assert!(!check(&later, KEY, id), "a longer expiry");
        assert!(!check(&good, "another-key", id), "another key");
        let sig = parse(&good).unwrap().2;
        assert!(!signed(KEY, ContactId::new(), 1_700_000_000, &sig), "another contact");
    }

    #[test]
    fn malformed_tokens_parse_to_nothing() {
        let id = ContactId::new();
        for bad in ["", "x", "a.b.c", &format!("{id}.1.zz"), &format!("{id}.1.00.00")] {
            assert!(parse(bad).is_none(), "{bad:?}");
        }
    }
}
