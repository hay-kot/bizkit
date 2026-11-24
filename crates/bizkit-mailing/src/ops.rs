//! The mailing operations: lists, contacts, broadcasts, preview and the send
//! log.

use std::collections::BTreeSet;
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::auth::{is_plausible_email, normalize_email};
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::jobs::Enqueue;
use bizkit_core::ops::{Access, Level, Method, NoContent, OpCtx, Operation, Scope, Success};
use bizkit_core::validate::{FieldErrors, Validate, check_name, present};
use bizkit_core::{ApiError, ModuleContext, Platform};
use http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::api::{
    Broadcast, BroadcastId, BroadcastList, BroadcastStatus, Contact, ContactCounts, ContactId,
    ContactPage, ContactStatus, DeliveryId, DeliveryKind, DeliveryStatus, ListId, MailDelivery,
    MailDeliveryPage, MailingList, MailingListList, Preview,
};
use crate::capture::FORM_KIND;
use crate::contacts::{self, NewContact, Source};
use crate::render::{self, Frame, Merge};
use crate::send::{self, FanOut};
use crate::store::{
    self, BroadcastRow, ContactFilter, ContactRow, DeliveryFilter, DeliveryRow, ListRow,
};

const READ: Access = Access::Workspace(Scope::new("mailing", Level::Read));
const WRITE: Access = Access::Workspace(Scope::new("mailing", Level::Write));
const ADMIN: Access = Access::Workspace(Scope::new("mailing", Level::Admin));
const TAGS: &[&str] = &["mailing"];

/// Test sends a workspace may queue per hour.
pub const TESTS_PER_HOUR: i64 = 20;
const MAX_TEXT: usize = 500;
const MAX_SUBJECT: usize = 200;
const MAX_BODY: usize = 100_000;
const MAX_TAGS: usize = 20;
const MAX_TAG: usize = 50;
const NOT_DRAFT: &str = "the broadcast was sent; only drafts change";

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.scope_area("mailing", "Mailing lists, contacts, broadcasts and sequences")?;
    cx.op(ListsList)?;
    cx.op(ListsCreate)?;
    cx.op(ListsGet)?;
    cx.op(ListsUpdate)?;
    cx.op(ListsDelete)?;
    cx.op(ContactsList)?;
    cx.op(ContactsCreate)?;
    cx.op(ContactsGet)?;
    cx.op(ContactsUpdate)?;
    cx.op(ContactsDelete)?;
    cx.op(ContactsUnsubscribe)?;
    cx.op(PreviewOp)?;
    cx.op(BroadcastsList)?;
    cx.op(BroadcastsCreate)?;
    cx.op(BroadcastsGet)?;
    cx.op(BroadcastsUpdate)?;
    cx.op(BroadcastsDelete)?;
    cx.op(BroadcastsTest)?;
    cx.op(BroadcastsSend)?;
    cx.op(DeliveriesList)?;
    Ok(())
}

// ---------------------------------------------------------------- conversions

fn count(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

/// The form IDs among `ids` that resolve to forms of `project_id`.
async fn live_forms(
    platform: &Platform,
    workspace_id: WorkspaceId,
    project_id: Option<ProjectId>,
    ids: &[String],
) -> anyhow::Result<BTreeSet<String>> {
    if ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let Some((_, resolver)) = platform.hook_target_kind(FORM_KIND) else {
        return Ok(BTreeSet::new());
    };
    let found = resolver.find(platform, workspace_id, ids).await.context("looking up forms")?;
    Ok(found
        .into_iter()
        .filter(|form| project_id.is_none_or(|p| form.project_id == p))
        .map(|form| form.id)
        .collect())
}

/// API lists for `rows`, with the links whose form still resolves.
async fn lists_to_api(
    platform: &Platform,
    workspace_id: WorkspaceId,
    rows: Vec<ListRow>,
) -> anyhow::Result<Vec<MailingList>> {
    let ids: Vec<ListId> = rows.iter().map(|row| row.id).collect();
    let links = store::list_forms(platform.db().read(), workspace_id, &ids).await?;
    let form_ids: Vec<String> = links.iter().map(|(_, form)| form.clone()).collect();
    let live = live_forms(platform, workspace_id, None, &form_ids).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let linked = links
                .iter()
                .filter(|(list, form)| *list == row.id && live.contains(form))
                .map(|(_, form)| form.clone())
                .collect();
            list_to_api(row, linked, None)
        })
        .collect())
}

fn list_to_api(row: ListRow, form_ids: Vec<String>, counts: Option<ContactCounts>) -> MailingList {
    MailingList {
        id: row.id,
        project_id: row.project_id,
        name: row.name,
        description: row.description,
        double_opt_in: row.double_opt_in,
        footer: row.footer,
        form_ids,
        counts,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// One list with its form links and counts, for get, create and update.
async fn full_list(platform: &Platform, row: ListRow) -> anyhow::Result<MailingList> {
    let db = platform.db().read();
    let counts = store::contact_counts(db, row.workspace_id, row.id).await?;
    let workspace_id = row.workspace_id;
    let mut list = lists_to_api(platform, workspace_id, vec![row])
        .await?
        .pop()
        .context("converting the list")?;
    list.counts = Some(ContactCounts {
        pending: count(counts.pending),
        subscribed: count(counts.subscribed),
        unsubscribed: count(counts.unsubscribed),
    });
    Ok(list)
}

fn contact_to_api(row: ContactRow) -> anyhow::Result<Contact> {
    Ok(Contact {
        tags: serde_json::from_str(&row.tags)
            .with_context(|| format!("decoding the tags of contact {}", row.id))?,
        status: ContactStatus::parse(&row.status)
            .with_context(|| format!("unknown contact status {:?}", row.status))?,
        id: row.id,
        list_id: row.list_id,
        email: row.email,
        name: row.name,
        source: row.source,
        subscribed_at: row.subscribed_at,
        unsubscribed_at: row.unsubscribed_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn broadcast_to_api(row: BroadcastRow) -> anyhow::Result<Broadcast> {
    Ok(Broadcast {
        status: BroadcastStatus::parse(&row.status)
            .with_context(|| format!("unknown broadcast status {:?}", row.status))?,
        id: row.id,
        list_id: row.list_id,
        subject: row.subject,
        body_md: row.body_md,
        tag: row.tag,
        recipients: count(row.recipients),
        sent_at: row.sent_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn delivery_to_api(row: DeliveryRow) -> anyhow::Result<MailDelivery> {
    Ok(MailDelivery {
        kind: DeliveryKind::parse(&row.kind)
            .with_context(|| format!("unknown delivery kind {:?}", row.kind))?,
        status: DeliveryStatus::parse(&row.status)
            .with_context(|| format!("unknown delivery status {:?}", row.status))?,
        id: row.id,
        contact_id: row.contact_id,
        broadcast_id: row.broadcast_id,
        to_email: row.to_email,
        subject: row.subject,
        error: row.error,
        attempts: u32::try_from(row.attempts).unwrap_or(0),
        created_at: row.created_at,
        sent_at: row.sent_at,
    })
}

async fn list_in(
    exec: impl sqlx::SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    list_id: ListId,
) -> Result<ListRow, ApiError> {
    store::find_list(exec, workspace_id, list_id).await?.ok_or(ApiError::NotFound)
}

async fn contact_in(
    exec: impl sqlx::SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    list_id: ListId,
    contact_id: ContactId,
) -> Result<ContactRow, ApiError> {
    store::find_contact(exec, workspace_id, contact_id)
        .await?
        .filter(|contact| contact.list_id == list_id)
        .ok_or(ApiError::NotFound)
}

async fn broadcast_in(
    exec: impl sqlx::SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    broadcast_id: BroadcastId,
) -> Result<BroadcastRow, ApiError> {
    store::find_broadcast(exec, workspace_id, broadcast_id).await?.ok_or(ApiError::NotFound)
}

fn field_error(field: &str, message: &str) -> ApiError {
    let mut errors = FieldErrors::default();
    errors.add(field, message);
    errors.into()
}

/// Checks that every form ID is a form of the list's project.
async fn check_forms(
    platform: &Platform,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    form_ids: &[String],
) -> Result<Vec<String>, ApiError> {
    let wanted: BTreeSet<String> = form_ids.iter().map(|id| id.trim().to_owned()).collect();
    let ids: Vec<String> = wanted.into_iter().collect();
    let live = live_forms(platform, workspace_id, Some(project_id), &ids).await?;
    if live.len() < ids.len() {
        return Err(field_error("form_ids", "must be forms of the list's project"));
    }
    Ok(live.into_iter().collect())
}

fn trimmed(text: Option<&String>) -> Option<String> {
    text.map(|text| text.trim().to_owned()).filter(|text| !text.is_empty())
}

fn check_text(errors: &mut FieldErrors, field: &str, text: Option<&String>, max: usize) {
    if text.is_some_and(|text| text.chars().count() > max) {
        errors.add(field, format!("must be at most {max} characters"));
    }
}

fn check_subject(errors: &mut FieldErrors, subject: &str) {
    if subject.trim().is_empty() {
        errors.add("subject", "must not be empty");
    } else if subject.chars().count() > MAX_SUBJECT {
        errors.add("subject", format!("must be at most {MAX_SUBJECT} characters"));
    }
}

fn check_body(errors: &mut FieldErrors, body: &str) {
    if body.len() > MAX_BODY {
        errors.add("body_md", format!("must be at most {MAX_BODY} bytes"));
    }
}

fn check_tags(errors: &mut FieldErrors, tags: &[String]) {
    if tags.len() > MAX_TAGS {
        errors.add("tags", format!("at most {MAX_TAGS} tags"));
    }
    if tags.iter().any(|tag| tag.trim().is_empty() || tag.chars().count() > MAX_TAG) {
        errors.add("tags", format!("each tag must be 1 to {MAX_TAG} characters"));
    }
}

/// Trimmed and without repeats, in the order given.
fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    tags.iter().map(|tag| tag.trim().to_owned()).filter(|tag| seen.insert(tag.clone())).collect()
}

fn check_limit(errors: &mut FieldErrors, limit: Option<u32>) {
    if limit.is_some_and(|limit| !(1..=200).contains(&limit)) {
        errors.add("limit", "must be between 1 and 200");
    }
}

/// Splits one extra row off a page of `limit + 1`.
fn page<T>(mut items: Vec<T>, limit: u32) -> (Vec<T>, bool) {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = items.len() > limit;
    items.truncate(limit);
    (items, more)
}

// ---------------------------------------------------------------- inputs

/// Which lists to show.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListLists {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// Only this project's lists.
    #[serde(default)]
    project_id: Option<ProjectId>,
}
impl Validate for ListLists {}

/// A new list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateList {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project it belongs to.
    project_id: ProjectId,
    /// The display name, shown in every mail.
    name: String,
    /// What subscribers get, for the team.
    #[serde(default)]
    description: Option<String>,
    /// Whether API adds must confirm by mail. Default true. Form captures
    /// always confirm.
    #[serde(default)]
    double_opt_in: Option<bool>,
    /// Text at the bottom of every mail, such as a postal address.
    #[serde(default)]
    footer: Option<String>,
    /// Forms of the project whose submitters join the list.
    #[serde(default)]
    form_ids: Vec<String>,
}

impl Validate for CreateList {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        check_text(&mut errors, "description", self.description.as_ref(), MAX_TEXT);
        check_text(&mut errors, "footer", self.footer.as_ref(), MAX_TEXT);
        errors.into_result()
    }
}

/// Changes to a list. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateList {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The list.
    list_id: ListId,
    /// The display name.
    #[serde(default)]
    name: Option<String>,
    /// What subscribers get; `null` removes it.
    #[serde(default, deserialize_with = "present")]
    #[expect(clippy::option_option, reason = "absent keeps the text, null removes it")]
    #[schema(value_type = Option<String>)]
    description: Option<Option<String>>,
    /// Whether API adds must confirm by mail.
    #[serde(default)]
    double_opt_in: Option<bool>,
    /// The mail footer; `null` removes it.
    #[serde(default, deserialize_with = "present")]
    #[expect(clippy::option_option, reason = "absent keeps the text, null removes it")]
    #[schema(value_type = Option<String>)]
    footer: Option<Option<String>>,
    /// Replaces the linked forms.
    #[serde(default)]
    form_ids: Option<Vec<String>>,
}

impl Validate for UpdateList {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(name) = &self.name {
            check_name(&mut errors, "name", name);
        }
        check_text(
            &mut errors,
            "description",
            self.description.as_ref().and_then(Option::as_ref),
            MAX_TEXT,
        );
        check_text(&mut errors, "footer", self.footer.as_ref().and_then(Option::as_ref), MAX_TEXT);
        errors.into_result()
    }
}

/// A list of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InList {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The list.
    list_id: ListId,
}
impl Validate for InList {}

/// Which contacts to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListContacts {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The list.
    list_id: ListId,
    /// Only contacts with this status.
    #[serde(default)]
    status: Option<ContactStatus>,
    /// Only contacts with this tag.
    #[serde(default)]
    tag: Option<String>,
    /// Only contacts whose address or name contains this text.
    #[serde(default)]
    q: Option<String>,
    /// Only contacts older than this one, for the next page.
    #[serde(default)]
    before: Option<ContactId>,
    /// At most this many, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}
impl Validate for ListContacts {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_limit(&mut errors, self.limit);
        errors.into_result()
    }
}

/// An address to add.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateContact {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The list.
    list_id: ListId,
    /// The address.
    email: String,
    /// A display name for `{{ name }}`.
    #[serde(default)]
    name: Option<String>,
    /// Labels for sending to part of the list.
    #[serde(default)]
    tags: Vec<String>,
}
impl Validate for CreateContact {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if !is_plausible_email(&normalize_email(&self.email)) {
            errors.add("email", "must be an email address");
        }
        check_text(&mut errors, "name", self.name.as_ref(), bizkit_core::contacts::MAX_NAME);
        check_tags(&mut errors, &self.tags);
        errors.into_result()
    }
}

/// Changes to a contact. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateContact {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The list.
    list_id: ListId,
    /// The contact.
    contact_id: ContactId,
    /// The display name; `null` removes it.
    #[serde(default, deserialize_with = "present")]
    #[expect(clippy::option_option, reason = "absent keeps the name, null removes it")]
    #[schema(value_type = Option<String>)]
    name: Option<Option<String>>,
    /// Replaces the tags.
    #[serde(default)]
    tags: Option<Vec<String>>,
}
impl Validate for UpdateContact {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_text(
            &mut errors,
            "name",
            self.name.as_ref().and_then(Option::as_ref),
            bizkit_core::contacts::MAX_NAME,
        );
        if let Some(tags) = &self.tags {
            check_tags(&mut errors, tags);
        }
        errors.into_result()
    }
}

/// A contact of a list.
#[expect(clippy::struct_field_names, reason = "path params are named like the path segments")]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InContact {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The list.
    list_id: ListId,
    /// The contact.
    contact_id: ContactId,
}
impl Validate for InContact {}

/// A mail to render.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreviewInput {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The list whose layout to use.
    list_id: ListId,
    /// The subject line.
    subject: String,
    /// The body in Markdown.
    body_md: String,
}
impl Validate for PreviewInput {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_body(&mut errors, &self.body_md);
        errors.into_result()
    }
}

/// Which broadcasts to list.
#[expect(clippy::struct_field_names, reason = "the IDs are named for what they identify")]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListBroadcasts {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// Only broadcasts to this project's lists.
    #[serde(default)]
    project_id: Option<ProjectId>,
    /// Only broadcasts to this list.
    #[serde(default)]
    list_id: Option<ListId>,
}
impl Validate for ListBroadcasts {}

/// A new draft broadcast.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateBroadcast {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The list it goes to.
    list_id: ListId,
    /// The subject line.
    subject: String,
    /// The body in Markdown.
    #[serde(default)]
    body_md: String,
    /// Only subscribers with this tag.
    #[serde(default)]
    tag: Option<String>,
}
impl Validate for CreateBroadcast {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_subject(&mut errors, &self.subject);
        check_body(&mut errors, &self.body_md);
        check_text(&mut errors, "tag", self.tag.as_ref(), MAX_TAG);
        errors.into_result()
    }
}

/// Changes to a draft. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateBroadcast {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The broadcast.
    broadcast_id: BroadcastId,
    /// The list it goes to.
    #[serde(default)]
    list_id: Option<ListId>,
    /// The subject line.
    #[serde(default)]
    subject: Option<String>,
    /// The body in Markdown.
    #[serde(default)]
    body_md: Option<String>,
    /// Only subscribers with this tag; `null` sends to all.
    #[serde(default, deserialize_with = "present")]
    #[expect(clippy::option_option, reason = "absent keeps the tag, null removes it")]
    #[schema(value_type = Option<String>)]
    tag: Option<Option<String>>,
}
impl Validate for UpdateBroadcast {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(subject) = &self.subject {
            check_subject(&mut errors, subject);
        }
        if let Some(body) = &self.body_md {
            check_body(&mut errors, body);
        }
        check_text(&mut errors, "tag", self.tag.as_ref().and_then(Option::as_ref), MAX_TAG);
        errors.into_result()
    }
}

/// A broadcast of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InBroadcast {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The broadcast.
    broadcast_id: BroadcastId,
}
impl Validate for InBroadcast {}

/// Where to send a test.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestBroadcast {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The broadcast.
    broadcast_id: BroadcastId,
    /// A workspace member's address.
    to: String,
}
impl Validate for TestBroadcast {}

/// Which deliveries to list. Give exactly one of `broadcast_id` and
/// `contact_id`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListDeliveries {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// A broadcast's deliveries.
    #[serde(default)]
    broadcast_id: Option<BroadcastId>,
    /// A contact's deliveries.
    #[serde(default)]
    contact_id: Option<ContactId>,
    /// Only deliveries with this status.
    #[serde(default)]
    status: Option<DeliveryStatus>,
    /// Only deliveries older than this one, for the next page.
    #[serde(default)]
    before: Option<DeliveryId>,
    /// At most this many, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}
impl Validate for ListDeliveries {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if self.broadcast_id.is_some() == self.contact_id.is_some() {
            errors.add("broadcast_id", "give exactly one of broadcast_id and contact_id");
        }
        check_limit(&mut errors, self.limit);
        errors.into_result()
    }
}

// ---------------------------------------------------------------- lists

struct ListsList;
impl Operation for ListsList {
    type Input = ListLists;
    type Output = MailingListList;
    const ID: &'static str = "mailing.lists.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/lists";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List the mailing lists of a workspace";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListLists) -> Result<MailingListList, ApiError> {
        let platform = cx.platform();
        let rows =
            store::list_lists(platform.db().read(), input.workspace_id, input.project_id).await?;
        Ok(MailingListList { items: lists_to_api(platform, input.workspace_id, rows).await? })
    }
}

struct ListsCreate;
impl Operation for ListsCreate {
    type Input = CreateList;
    type Output = MailingList;
    const ID: &'static str = "mailing.lists.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/lists";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Create a mailing list in a project";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateList) -> Result<MailingList, ApiError> {
        let platform = cx.platform();
        let form_ids =
            check_forms(platform, input.workspace_id, input.project_id, &input.form_ids).await?;
        let now = cx.now();
        let row = ListRow {
            id: ListId::new(),
            workspace_id: input.workspace_id,
            project_id: input.project_id,
            name: input.name.trim().to_owned(),
            description: trimmed(input.description.as_ref()),
            double_opt_in: input.double_opt_in.unwrap_or(true),
            footer: trimmed(input.footer.as_ref()),
            created_at: now,
            updated_at: now,
        };
        let mut tx = cx.write().await?;
        if !bizkit_core::projects::exists(&mut *tx, input.workspace_id, input.project_id).await? {
            return Err(field_error("project_id", "is not a project of this workspace"));
        }
        store::insert_list(&mut *tx, &row).await?;
        store::set_list_forms(&mut tx, row.workspace_id, row.id, &form_ids).await?;
        tx.commit().await?;
        Ok(full_list(platform, row).await?)
    }
}

struct ListsGet;
impl Operation for ListsGet {
    type Input = InList;
    type Output = MailingList;
    const ID: &'static str = "mailing.lists.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/lists/{list_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a mailing list with its contact counts";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InList) -> Result<MailingList, ApiError> {
        let row = list_in(cx.platform().db().read(), input.workspace_id, input.list_id).await?;
        Ok(full_list(cx.platform(), row).await?)
    }
}

struct ListsUpdate;
impl Operation for ListsUpdate {
    type Input = UpdateList;
    type Output = MailingList;
    const ID: &'static str = "mailing.lists.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/lists/{list_id}";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Change a mailing list's settings and linked forms";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateList) -> Result<MailingList, ApiError> {
        let platform = cx.platform();
        let current = list_in(platform.db().read(), input.workspace_id, input.list_id).await?;
        // Resolving forms reads other modules' tables, so it happens before
        // the write transaction.
        let form_ids = match &input.form_ids {
            Some(ids) => {
                Some(check_forms(platform, input.workspace_id, current.project_id, ids).await?)
            }
            None => None,
        };
        let mut tx = cx.write().await?;
        let mut row = list_in(&mut *tx, input.workspace_id, input.list_id).await?;
        if let Some(name) = input.name {
            name.trim().clone_into(&mut row.name);
        }
        if let Some(description) = input.description {
            row.description = trimmed(description.as_ref());
        }
        if let Some(double_opt_in) = input.double_opt_in {
            row.double_opt_in = double_opt_in;
        }
        if let Some(footer) = input.footer {
            row.footer = trimmed(footer.as_ref());
        }
        row.updated_at = cx.now();
        store::update_list(&mut *tx, &row).await?;
        if let Some(form_ids) = form_ids {
            store::set_list_forms(&mut tx, row.workspace_id, row.id, &form_ids).await?;
        }
        tx.commit().await?;
        Ok(full_list(platform, row).await?)
    }
}

struct ListsDelete;
impl Operation for ListsDelete {
    type Input = InList;
    type Output = NoContent;
    const ID: &'static str = "mailing.lists.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/lists/{list_id}";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str =
        "Delete a mailing list with its contacts, broadcasts and send log";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InList) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let deleted = store::delete_list(&mut *tx, input.workspace_id, input.list_id).await?;
        tx.commit().await?;
        if deleted { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

// ---------------------------------------------------------------- contacts

struct ContactsList;
impl Operation for ContactsList {
    type Input = ListContacts;
    type Output = ContactPage;
    const ID: &'static str = "mailing.contacts.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/lists/{list_id}/contacts";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List a mailing list's contacts, newest first";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListContacts) -> Result<ContactPage, ApiError> {
        let db = cx.platform().db().read();
        list_in(db, input.workspace_id, input.list_id).await?;
        let limit = input.limit.unwrap_or(50);
        let q = input.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
        let filter = ContactFilter {
            status: input.status.map(ContactStatus::as_str),
            tag: input.tag.as_deref(),
            q,
            before: input.before,
        };
        let rows = store::list_contacts(
            db,
            input.workspace_id,
            input.list_id,
            &filter,
            i64::from(limit) + 1,
        )
        .await?;
        let items = rows.into_iter().map(contact_to_api).collect::<anyhow::Result<Vec<_>>>()?;
        let (items, more) = page(items, limit);
        let next_before = if more { items.last().map(|c| c.id) } else { None };
        Ok(ContactPage { items, next_before })
    }
}

struct ContactsCreate;
impl Operation for ContactsCreate {
    type Input = CreateContact;
    type Output = Contact;
    const ID: &'static str = "mailing.contacts.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/lists/{list_id}/contacts";
    const ACCESS: Access = WRITE;
    // Pending with a confirm mail when the list has double opt-in, else subscribed.
    const SUMMARY: &'static str = "Add an address to a mailing list";
    const SUCCESS: Success = Success::Created;
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateContact) -> Result<Contact, ApiError> {
        let email = normalize_email(&input.email);
        let source = if cx.user().is_ok() { Source::Admin } else { Source::Api };
        let mut tx = cx.write().await?;
        let list = list_in(&mut *tx, input.workspace_id, input.list_id).await?;
        if store::find_contact_by_email(&mut *tx, list.workspace_id, list.id, &email)
            .await?
            .is_some()
        {
            return Err(ApiError::Conflict("the address is already on the list".to_owned()));
        }
        let new = NewContact {
            email,
            name: trimmed(input.name.as_ref()),
            tags: clean_tags(&input.tags),
            source,
        };
        let contact = contacts::subscribe(cx.platform(), &mut tx, &list, new).await?;
        tx.commit().await?;
        Ok(contact_to_api(contact)?)
    }
}

struct ContactsGet;
impl Operation for ContactsGet {
    type Input = InContact;
    type Output = Contact;
    const ID: &'static str = "mailing.contacts.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/mailing/lists/{list_id}/contacts/{contact_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a contact";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InContact) -> Result<Contact, ApiError> {
        let row = contact_in(
            cx.platform().db().read(),
            input.workspace_id,
            input.list_id,
            input.contact_id,
        )
        .await?;
        Ok(contact_to_api(row)?)
    }
}

struct ContactsUpdate;
impl Operation for ContactsUpdate {
    type Input = UpdateContact;
    type Output = Contact;
    const ID: &'static str = "mailing.contacts.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/mailing/lists/{list_id}/contacts/{contact_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Change a contact's name or tags";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateContact) -> Result<Contact, ApiError> {
        let mut tx = cx.write().await?;
        let mut row =
            contact_in(&mut *tx, input.workspace_id, input.list_id, input.contact_id).await?;
        if let Some(name) = input.name {
            row.name = trimmed(name.as_ref());
        }
        if let Some(tags) = input.tags {
            row.tags = serde_json::to_string(&clean_tags(&tags)).map_err(anyhow::Error::from)?;
        }
        row.updated_at = cx.now();
        store::update_contact(&mut *tx, &row).await?;
        tx.commit().await?;
        Ok(contact_to_api(row)?)
    }
}

struct ContactsDelete;
impl Operation for ContactsDelete {
    type Input = InContact;
    type Output = NoContent;
    const ID: &'static str = "mailing.contacts.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/mailing/lists/{list_id}/contacts/{contact_id}";
    const ACCESS: Access = WRITE;
    // Unsubscribed contacts stay: the row is what keeps the address from
    // getting mail again.
    const SUMMARY: &'static str = "Delete a pending or subscribed contact";
    const SUCCESS: Success = Success::NoContent;
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InContact) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let row = contact_in(&mut *tx, input.workspace_id, input.list_id, input.contact_id).await?;
        if row.status == ContactStatus::Unsubscribed.as_str() {
            return Err(ApiError::Conflict(
                "unsubscribed contacts stay, so the address gets no more mail".to_owned(),
            ));
        }
        store::delete_contact(&mut *tx, input.workspace_id, input.contact_id).await?;
        tx.commit().await?;
        Ok(NoContent)
    }
}

struct ContactsUnsubscribe;
impl Operation for ContactsUnsubscribe {
    type Input = InContact;
    type Output = Contact;
    const ID: &'static str = "mailing.contacts.unsubscribe";
    const METHOD: Method = Method::Post;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/mailing/lists/{list_id}/contacts/{contact_id}/unsubscribe";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Unsubscribe a contact for good";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InContact) -> Result<Contact, ApiError> {
        let mut tx = cx.write().await?;
        let mut row =
            contact_in(&mut *tx, input.workspace_id, input.list_id, input.contact_id).await?;
        contacts::unsubscribe(&mut tx, &mut row, cx.now()).await?;
        tx.commit().await?;
        Ok(contact_to_api(row)?)
    }
}

// ---------------------------------------------------------------- preview

struct PreviewOp;
impl Operation for PreviewOp {
    type Input = PreviewInput;
    type Output = Preview;
    const ID: &'static str = "mailing.preview";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/preview";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Render a mail in a list's layout without sending it";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: PreviewInput) -> Result<Preview, ApiError> {
        let platform = cx.platform();
        let list = list_in(platform.db().read(), input.workspace_id, input.list_id).await?;
        let unsubscribe = crate::public::unsubscribe_url(platform, "preview")?;
        let frame = Frame {
            list_name: &list.name,
            footer: list.footer.as_deref(),
            unsubscribe: Some(&unsubscribe),
        };
        let merge = Merge { name: Some("Ada Lovelace"), email: "ada@example.com" };
        let mail = render::render(&frame, &input.subject, &input.body_md, &merge);
        Ok(Preview { subject: mail.subject, html: mail.html, text: mail.text })
    }
}

// ---------------------------------------------------------------- broadcasts

struct BroadcastsList;
impl Operation for BroadcastsList {
    type Input = ListBroadcasts;
    type Output = BroadcastList;
    const ID: &'static str = "mailing.broadcasts.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/broadcasts";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List broadcasts, newest first";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListBroadcasts) -> Result<BroadcastList, ApiError> {
        let rows = store::list_broadcasts(
            cx.platform().db().read(),
            input.workspace_id,
            input.project_id,
            input.list_id,
        )
        .await?;
        let items = rows.into_iter().map(broadcast_to_api).collect::<anyhow::Result<_>>()?;
        Ok(BroadcastList { items })
    }
}

struct BroadcastsCreate;
impl Operation for BroadcastsCreate {
    type Input = CreateBroadcast;
    type Output = Broadcast;
    const ID: &'static str = "mailing.broadcasts.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/broadcasts";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Create a draft broadcast to a list";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateBroadcast) -> Result<Broadcast, ApiError> {
        let mut tx = cx.write().await?;
        let Some(list) = store::find_list(&mut *tx, input.workspace_id, input.list_id).await?
        else {
            return Err(field_error("list_id", "is not a list of this workspace"));
        };
        let now = cx.now();
        let row = BroadcastRow {
            id: BroadcastId::new(),
            workspace_id: list.workspace_id,
            list_id: list.id,
            subject: input.subject.trim().to_owned(),
            body_md: input.body_md,
            tag: trimmed(input.tag.as_ref()),
            status: BroadcastStatus::Draft.as_str().to_owned(),
            recipients: 0,
            sent_at: None,
            created_at: now,
            updated_at: now,
        };
        store::insert_broadcast(&mut *tx, &row).await?;
        tx.commit().await?;
        Ok(broadcast_to_api(row)?)
    }
}

struct BroadcastsGet;
impl Operation for BroadcastsGet {
    type Input = InBroadcast;
    type Output = Broadcast;
    const ID: &'static str = "mailing.broadcasts.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/broadcasts/{broadcast_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a broadcast";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InBroadcast) -> Result<Broadcast, ApiError> {
        let row =
            broadcast_in(cx.platform().db().read(), input.workspace_id, input.broadcast_id).await?;
        Ok(broadcast_to_api(row)?)
    }
}

struct BroadcastsUpdate;
impl Operation for BroadcastsUpdate {
    type Input = UpdateBroadcast;
    type Output = Broadcast;
    const ID: &'static str = "mailing.broadcasts.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/broadcasts/{broadcast_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Change a draft broadcast";
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateBroadcast) -> Result<Broadcast, ApiError> {
        let mut tx = cx.write().await?;
        let mut row = broadcast_in(&mut *tx, input.workspace_id, input.broadcast_id).await?;
        if row.status != BroadcastStatus::Draft.as_str() {
            return Err(ApiError::Conflict(NOT_DRAFT.to_owned()));
        }
        if let Some(list_id) = input.list_id {
            if store::find_list(&mut *tx, input.workspace_id, list_id).await?.is_none() {
                return Err(field_error("list_id", "is not a list of this workspace"));
            }
            row.list_id = list_id;
        }
        if let Some(subject) = input.subject {
            subject.trim().clone_into(&mut row.subject);
        }
        if let Some(body) = input.body_md {
            row.body_md = body;
        }
        if let Some(tag) = input.tag {
            row.tag = trimmed(tag.as_ref());
        }
        row.updated_at = cx.now();
        store::update_broadcast(&mut *tx, &row).await?;
        tx.commit().await?;
        Ok(broadcast_to_api(row)?)
    }
}

struct BroadcastsDelete;
impl Operation for BroadcastsDelete {
    type Input = InBroadcast;
    type Output = NoContent;
    const ID: &'static str = "mailing.broadcasts.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/broadcasts/{broadcast_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Delete a draft broadcast";
    const SUCCESS: Success = Success::NoContent;
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InBroadcast) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let row = broadcast_in(&mut *tx, input.workspace_id, input.broadcast_id).await?;
        if row.status != BroadcastStatus::Draft.as_str() {
            return Err(ApiError::Conflict(NOT_DRAFT.to_owned()));
        }
        store::delete_broadcast(&mut *tx, input.workspace_id, input.broadcast_id).await?;
        tx.commit().await?;
        Ok(NoContent)
    }
}

struct BroadcastsTest;
impl Operation for BroadcastsTest {
    type Input = TestBroadcast;
    type Output = MailDelivery;
    const ID: &'static str = "mailing.broadcasts.test";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/broadcasts/{broadcast_id}/test";
    const ACCESS: Access = WRITE;
    // Only to members, so the endpoint can't mail anyone.
    const SUMMARY: &'static str = "Send a broadcast once to a workspace member";
    const SUCCESS: Success = Success::Created;
    const ERRORS: &'static [StatusCode] = &[StatusCode::TOO_MANY_REQUESTS];
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: TestBroadcast) -> Result<MailDelivery, ApiError> {
        let platform = cx.platform();
        let to = normalize_email(&input.to);
        let mut tx = cx.write().await?;
        let broadcast = broadcast_in(&mut *tx, input.workspace_id, input.broadcast_id).await?;
        if !bizkit_core::members::is_member_email(&mut *tx, input.workspace_id, &to).await? {
            return Err(field_error("to", "must be the address of a workspace member"));
        }
        let hour_ago = cx.now().minus(Duration::from_hours(1));
        if store::tests_since(&mut *tx, input.workspace_id, hour_ago).await? >= TESTS_PER_HOUR {
            return Err(ApiError::RateLimited { retry_after_secs: 300 });
        }
        let list = list_in(&mut *tx, input.workspace_id, broadcast.list_id).await?;
        let subject = format!("[Test] {}", broadcast.subject);
        let mut row = send::delivery(platform, &list, DeliveryKind::Test, &to, subject);
        row.broadcast_id = Some(broadcast.id);
        send::queue(platform, &mut tx, &row).await?;
        tx.commit().await?;
        Ok(delivery_to_api(row)?)
    }
}

struct BroadcastsSend;
impl Operation for BroadcastsSend {
    type Input = InBroadcast;
    type Output = Broadcast;
    const ID: &'static str = "mailing.broadcasts.send";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/broadcasts/{broadcast_id}/send";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Send a draft broadcast to its list's subscribers";
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InBroadcast) -> Result<Broadcast, ApiError> {
        let platform = cx.platform();
        let mut tx = cx.write().await?;
        let mut row = broadcast_in(&mut *tx, input.workspace_id, input.broadcast_id).await?;
        if row.status != BroadcastStatus::Draft.as_str() {
            return Err(ApiError::Conflict(NOT_DRAFT.to_owned()));
        }
        let now = cx.now();
        BroadcastStatus::Sending.as_str().clone_into(&mut row.status);
        row.sent_at = Some(now);
        row.updated_at = now;
        store::update_broadcast(&mut *tx, &row).await?;
        let job = FanOut { workspace_id: row.workspace_id, broadcast_id: row.id };
        platform.bus().enqueue(&mut tx, &job, Enqueue::default()).await?;
        tx.commit().await?;
        Ok(broadcast_to_api(row)?)
    }
}

// ---------------------------------------------------------------- deliveries

struct DeliveriesList;
impl Operation for DeliveriesList {
    type Input = ListDeliveries;
    type Output = MailDeliveryPage;
    const ID: &'static str = "mailing.deliveries.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/deliveries";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List the send log of a broadcast or a contact, newest first";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListDeliveries) -> Result<MailDeliveryPage, ApiError> {
        let limit = input.limit.unwrap_or(50);
        let filter = DeliveryFilter {
            broadcast_id: input.broadcast_id,
            contact_id: input.contact_id,
            status: input.status.map(DeliveryStatus::as_str),
            before: input.before,
        };
        let rows = store::list_deliveries(
            cx.platform().db().read(),
            input.workspace_id,
            &filter,
            i64::from(limit) + 1,
        )
        .await?;
        let items = rows.into_iter().map(delivery_to_api).collect::<anyhow::Result<Vec<_>>>()?;
        let (items, more) = page(items, limit);
        let next_before = if more { items.last().map(|d| d.id) } else { None };
        Ok(MailDeliveryPage { items, next_before })
    }
}
