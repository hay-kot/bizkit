//! The workspace-wide suppression list: addresses no list mails, whatever
//! their contact rows say. Bounces and complaints from the provider add to
//! it, and so do admins.

use anyhow::Context as _;
use bizkit_core::db::WriteTx;
use bizkit_core::ids::WorkspaceId;
use bizkit_core::time::UnixMillis;
use sha2::{Digest as _, Sha256};
use sqlx::SqliteExecutor;

use crate::api::{DeliveryId, SuppressionReason};
use crate::contacts;
use crate::store::{self, SuppressionRow};

/// Why an address must not get mail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Block {
    /// It is on the suppression list.
    Suppressed(SuppressionReason),
    /// It was erased on request. Erased addresses can't be added either.
    Erased,
}

impl Block {
    /// The reason text a skipped delivery records.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Suppressed(reason) => reason.as_str(),
            Self::Erased => "erased",
        }
    }
}

/// Why `email` must not get mail in the workspace, or `None` when it may.
pub(crate) async fn blocked(
    exec: impl SqliteExecutor<'_> + Copy,
    workspace_id: WorkspaceId,
    email: &str,
) -> sqlx::Result<Option<Block>> {
    let email = email.to_lowercase();
    if store::is_erased(exec, workspace_id, &erasure_hash(workspace_id, &email)).await? {
        return Ok(Some(Block::Erased));
    }
    let row = store::find_suppression(exec, workspace_id, &email).await?;
    Ok(row.and_then(|r| SuppressionReason::parse(&r.reason)).map(Block::Suppressed))
}

/// Whether `email` was erased from the workspace.
pub(crate) async fn erased(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    email: &str,
) -> sqlx::Result<bool> {
    store::is_erased(exec, workspace_id, &erasure_hash(workspace_id, &email.to_lowercase())).await
}

/// The stored hash of an erased address: SHA-256 hex of the workspace ID, a
/// NUL and the lower-case address. The workspace ID keeps two workspaces'
/// records from matching each other.
pub(crate) fn erasure_hash(workspace_id: WorkspaceId, email: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(workspace_id.to_string().as_bytes());
    hasher.update([0]);
    hasher.update(email.to_lowercase().as_bytes());
    hex::encode(hasher.finalize())
}

/// What to record for a suppression.
#[derive(Debug, Clone)]
pub(crate) struct Suppress<'a> {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) email: &'a str,
    pub(crate) reason: SuppressionReason,
    pub(crate) delivery_id: Option<DeliveryId>,
    pub(crate) note: Option<String>,
    pub(crate) now: UnixMillis,
}

/// Adds the address to the list; true when it wasn't there (or a complaint
/// replaced a weaker reason). An erased address is left out: its hash
/// already blocks it, and a row would store it in plain text again. A complaint also unsubscribes every contact
/// row with the address in the workspace: a spam report withdraws consent.
/// A bounce doesn't, since the address may work again.
pub(crate) async fn suppress(tx: &mut WriteTx, entry: Suppress<'_>) -> anyhow::Result<bool> {
    let email = entry.email.to_lowercase();
    if erased(&mut **tx, entry.workspace_id, &email).await.context("checking for an erasure")? {
        return Ok(false);
    }
    let row = SuppressionRow {
        workspace_id: entry.workspace_id,
        email: email.clone(),
        reason: entry.reason.as_str().to_owned(),
        delivery_id: entry.delivery_id,
        note: entry.note,
        created_at: entry.now,
    };
    let added = store::upsert_suppression(&mut **tx, &row).await.context("adding a suppression")?;
    if entry.reason == SuppressionReason::Complaint {
        let ids = store::contacts_by_email(&mut **tx, entry.workspace_id, &email)
            .await
            .context("finding the address's contacts")?;
        for id in ids {
            let contact = store::find_contact(&mut **tx, entry.workspace_id, id)
                .await
                .context("loading a contact")?;
            if let Some(mut contact) = contact {
                contacts::unsubscribe(tx, &mut contact, entry.now).await?;
            }
        }
    }
    Ok(added)
}
