//! The suppression operations. Suppressions belong to the workspace, not a
//! list or project: a bounce on one list stops mail from every list.

use anyhow::Context as _;
use bizkit_core::auth::{is_plausible_email, normalize_email};
use bizkit_core::ids::WorkspaceId;
use bizkit_core::ops::{Method, NoContent, OpCtx, Operation, Success};
use bizkit_core::validate::{FieldErrors, Validate};
use bizkit_core::{ApiError, ModuleContext};
use http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::api::{Suppression, SuppressionPage, SuppressionReason};
use crate::ops::{ADMIN, READ, TAGS, check_limit, check_text, page, trimmed};
use crate::store::{self, SuppressionFilter, SuppressionRow};
use crate::suppress::{self, Suppress};

const MAX_NOTE: usize = 500;

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.op(SuppressionsList)?;
    cx.op(SuppressionsCreate)?;
    cx.op(SuppressionsDelete)?;
    Ok(())
}

fn suppression_to_api(row: SuppressionRow) -> anyhow::Result<Suppression> {
    Ok(Suppression {
        reason: SuppressionReason::parse(&row.reason)
            .with_context(|| format!("unknown suppression reason {:?}", row.reason))?,
        email: row.email,
        delivery_id: row.delivery_id,
        note: row.note,
        created_at: row.created_at,
    })
}

/// Which suppressions to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListSuppressions {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// Only addresses that contain this text.
    #[serde(default)]
    q: Option<String>,
    /// Only suppressions with this reason.
    #[serde(default)]
    reason: Option<SuppressionReason>,
    /// Only addresses after this one, for the next page.
    #[serde(default)]
    after: Option<String>,
    /// At most this many, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}
impl Validate for ListSuppressions {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_limit(&mut errors, self.limit);
        errors.into_result()
    }
}

/// An address to suppress.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateSuppression {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The address.
    email: String,
    /// Why, for the team.
    #[serde(default)]
    note: Option<String>,
}
impl Validate for CreateSuppression {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if !is_plausible_email(&normalize_email(&self.email)) {
            errors.add("email", "must be an email address");
        }
        check_text(&mut errors, "note", self.note.as_ref(), MAX_NOTE);
        errors.into_result()
    }
}

/// A suppressed address.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InSuppression {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The address.
    email: String,
}
impl Validate for InSuppression {}

struct SuppressionsList;
impl Operation for SuppressionsList {
    type Input = ListSuppressions;
    type Output = SuppressionPage;
    const ID: &'static str = "mailing.suppressions.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/suppressions";
    const ACCESS: bizkit_core::ops::Access = READ;
    const SUMMARY: &'static str = "List the workspace's suppressed addresses";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListSuppressions) -> Result<SuppressionPage, ApiError> {
        let limit = input.limit.unwrap_or(50);
        let q = input.q.as_deref().map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
        let filter = SuppressionFilter {
            q: q.as_deref(),
            reason: input.reason.map(SuppressionReason::as_str),
            after: input.after.as_deref(),
        };
        let rows = store::list_suppressions(
            cx.platform().db().read(),
            input.workspace_id,
            &filter,
            i64::from(limit) + 1,
        )
        .await?;
        let items = rows.into_iter().map(suppression_to_api).collect::<anyhow::Result<Vec<_>>>()?;
        let (items, more) = page(items, limit);
        let next_after = if more { items.last().map(|s| s.email.clone()) } else { None };
        Ok(SuppressionPage { items, next_after })
    }
}

struct SuppressionsCreate;
impl Operation for SuppressionsCreate {
    type Input = CreateSuppression;
    type Output = Suppression;
    const ID: &'static str = "mailing.suppressions.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/suppressions";
    const ACCESS: bizkit_core::ops::Access = ADMIN;
    // Stops all mail to the address from every list in the workspace.
    const SUMMARY: &'static str = "Suppress an address in the workspace";
    const SUCCESS: Success = Success::Created;
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateSuppression) -> Result<Suppression, ApiError> {
        let email = normalize_email(&input.email);
        let now = cx.now();
        let mut tx = cx.write().await?;
        let entry = Suppress {
            workspace_id: input.workspace_id,
            email: &email,
            reason: SuppressionReason::Manual,
            delivery_id: None,
            note: trimmed(input.note.as_ref()),
            now,
        };
        if !suppress::suppress(&mut tx, entry).await? {
            return Err(ApiError::Conflict("the address is already suppressed".to_owned()));
        }
        let row = store::find_suppression(&mut *tx, input.workspace_id, &email)
            .await?
            .context("reading the new suppression")?;
        tx.commit().await?;
        Ok(suppression_to_api(row)?)
    }
}

struct SuppressionsDelete;
impl Operation for SuppressionsDelete {
    type Input = InSuppression;
    type Output = NoContent;
    const ID: &'static str = "mailing.suppressions.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/suppressions/{email}";
    const ACCESS: bizkit_core::ops::Access = ADMIN;
    // A complaint stays: the recipient said no. Removing a bounce or manual
    // entry re-subscribes nothing; mail flows only to rows still subscribed.
    const SUMMARY: &'static str = "Remove a bounce or manual suppression";
    const SUCCESS: Success = Success::NoContent;
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InSuppression) -> Result<NoContent, ApiError> {
        let email = normalize_email(&input.email);
        let mut tx = cx.write().await?;
        let row = store::find_suppression(&mut *tx, input.workspace_id, &email)
            .await?
            .ok_or(ApiError::NotFound)?;
        if row.reason == SuppressionReason::Complaint.as_str() {
            return Err(ApiError::Conflict(
                "a complaint stays: the recipient reported the mail as spam".to_owned(),
            ));
        }
        store::delete_suppression(&mut *tx, input.workspace_id, &email).await?;
        tx.commit().await?;
        Ok(NoContent)
    }
}
