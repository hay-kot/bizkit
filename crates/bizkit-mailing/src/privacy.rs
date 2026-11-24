//! Export and erase for one address: what mailing holds on it in a
//! workspace, and its removal. Erase leaves only a hash
//! (`mailing_erasures`), which keeps the address from mail and lists.

use std::collections::BTreeMap;

use anyhow::Context as _;
use bizkit_core::auth::{is_plausible_email, normalize_email};
use bizkit_core::db::WriteTx;
use bizkit_core::ids::WorkspaceId;
use bizkit_core::ops::{Method, OpCtx, Operation};
use bizkit_core::time::UnixMillis;
use bizkit_core::validate::{FieldErrors, Validate};
use bizkit_core::{ApiError, ModuleContext};
use serde::Deserialize;
use sqlx::SqliteConnection;
use utoipa::ToSchema;

use crate::api::{AddressExport, AddressList, ErasedAddress};
use crate::ops::{ADMIN, TAGS, contact_to_api, delivery_to_api};
use crate::sequence_ops::enrollment_to_api;
use crate::store;
use crate::suppress::{self, erasure_hash};
use crate::suppression_ops::suppression_to_api;

/// The pause reason that replaces one quoting an erased address.
const REDACTED_PAUSE: &str = "Deliveries failed in a row; the last error named an erased address.";

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.op(AddressesExport)?;
    cx.op(AddressesErase)?;
    Ok(())
}

/// An address in the workspace. It goes in the body, not the path, so it
/// stays out of request logs.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InAddress {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The address.
    email: String,
}
impl Validate for InAddress {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if !is_plausible_email(&normalize_email(&self.email)) {
            errors.add("email", "must be an email address");
        }
        errors.into_result()
    }
}

/// Everything mailing holds on the lower-case `email` in the workspace.
pub(crate) async fn export(
    conn: &mut SqliteConnection,
    workspace_id: WorkspaceId,
    email: &str,
) -> anyhow::Result<AddressExport> {
    let rows = store::address_contacts(&mut *conn, workspace_id, email, None)
        .await
        .context("loading the address's contacts")?;
    let mut lists = BTreeMap::new();
    let mut contacts = Vec::with_capacity(rows.len());
    for row in rows {
        lists.entry(row.contact.list_id).or_insert_with(|| AddressList {
            id: row.contact.list_id,
            project_id: row.project_id,
            name: row.list_name,
        });
        contacts.push(contact_to_api(row.contact)?);
    }
    let enrollments = store::address_enrollments(&mut *conn, workspace_id, email)
        .await
        .context("loading the address's sequence runs")?
        .into_iter()
        .map(enrollment_to_api)
        .collect::<anyhow::Result<Vec<_>>>()?;
    let deliveries = store::address_deliveries(&mut *conn, workspace_id, email)
        .await
        .context("loading the address's deliveries")?
        .into_iter()
        .map(delivery_to_api)
        .collect::<anyhow::Result<Vec<_>>>()?;
    let suppression = store::find_suppression(&mut *conn, workspace_id, email)
        .await
        .context("loading the address's suppression")?
        .map(suppression_to_api)
        .transpose()?;
    let erased = suppress::erased(&mut *conn, workspace_id, email)
        .await
        .context("checking for an erasure")?;
    Ok(AddressExport {
        email: email.to_owned(),
        erased,
        suppression,
        lists: lists.into_values().collect(),
        contacts,
        enrollments,
        deliveries,
    })
}

/// Deletes everything mailing holds on the lower-case `email` in the
/// workspace and records its hash. Deliveries go first, so no row is ever
/// left with the address and no contact.
pub(crate) async fn erase(
    tx: &mut WriteTx,
    workspace_id: WorkspaceId,
    email: &str,
    now: UnixMillis,
) -> anyhow::Result<ErasedAddress> {
    let deliveries = store::delete_address_deliveries(&mut **tx, workspace_id, email)
        .await
        .context("deleting the address's deliveries")?;
    let contacts = store::delete_address_contacts(&mut **tx, workspace_id, email)
        .await
        .context("deleting the address's contacts")?;
    let suppression = store::delete_suppression(&mut **tx, workspace_id, email)
        .await
        .context("deleting the address's suppression")?;
    store::redact_pause_reasons(&mut **tx, workspace_id, email, REDACTED_PAUSE)
        .await
        .context("redacting pause reasons")?;
    store::insert_erasure(&mut **tx, workspace_id, &erasure_hash(workspace_id, email), now)
        .await
        .context("recording the erasure")?;
    Ok(ErasedAddress { contacts, deliveries, suppression })
}

struct AddressesExport;
impl Operation for AddressesExport {
    type Input = InAddress;
    type Output = AddressExport;
    const ID: &'static str = "mailing.addresses.export";
    // A POST so the address travels in the body, not a logged URL.
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/addresses/export";
    const ACCESS: bizkit_core::ops::Access = ADMIN;
    const SUMMARY: &'static str =
        "Export everything mailing holds on an address in the workspace (GDPR access)";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InAddress) -> Result<AddressExport, ApiError> {
        let email = normalize_email(&input.email);
        let mut conn = cx.platform().db().read().acquire().await.map_err(anyhow::Error::from)?;
        Ok(export(&mut conn, input.workspace_id, &email).await?)
    }
}

struct AddressesErase;
impl Operation for AddressesErase {
    type Input = InAddress;
    type Output = ErasedAddress;
    const ID: &'static str = "mailing.addresses.erase";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/addresses/erase";
    const ACCESS: bizkit_core::ops::Access = ADMIN;
    // Contacts, sequence runs, send log and suppression go; a hash stays so
    // the address is never mailed or added again. Can't be undone.
    const SUMMARY: &'static str =
        "Erase an address from every list and the send log of the workspace (GDPR erasure)";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InAddress) -> Result<ErasedAddress, ApiError> {
        let email = normalize_email(&input.email);
        let now = cx.now();
        let mut tx = cx.write().await?;
        let erased = erase(&mut tx, input.workspace_id, &email, now).await?;
        tx.commit().await?;
        Ok(erased)
    }
}
