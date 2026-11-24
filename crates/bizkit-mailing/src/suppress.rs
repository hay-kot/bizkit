//! The workspace-wide suppression list: addresses no list mails, whatever
//! their contact rows say. Bounces and complaints from the provider add to
//! it, and so do admins.

use anyhow::Context as _;
use bizkit_core::db::WriteTx;
use bizkit_core::ids::WorkspaceId;
use bizkit_core::time::UnixMillis;
use sqlx::SqliteExecutor;

use crate::api::{DeliveryId, SuppressionReason};
use crate::contacts;
use crate::store::{self, SuppressionRow};

/// Why `email` must not get mail in the workspace, or `None` when it may.
pub(crate) async fn suppressed(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    email: &str,
) -> sqlx::Result<Option<SuppressionReason>> {
    let row = store::find_suppression(exec, workspace_id, &email.to_lowercase()).await?;
    Ok(row.and_then(|r| SuppressionReason::parse(&r.reason)))
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
/// replaced a weaker reason). A complaint also unsubscribes every contact
/// row with the address in the workspace: a spam report withdraws consent.
/// A bounce doesn't, since the address may work again.
pub(crate) async fn suppress(tx: &mut WriteTx, entry: Suppress<'_>) -> anyhow::Result<bool> {
    let email = entry.email.to_lowercase();
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
