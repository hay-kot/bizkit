//! The forms operations: form CRUD, submissions and CSV export.

use std::collections::BTreeMap;

use anyhow::Context as _;
use bizkit_core::auth::{is_plausible_email, normalize_email, random_token};
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Level, Method, NoContent, OpCtx, Operation, Scope, Success};
use bizkit_core::validate::{FieldErrors, Validate, check_name};
use bizkit_core::{ApiError, ModuleContext, Platform};
use serde::{Deserialize, Deserializer};
use url::Url;
use utoipa::ToSchema;

use crate::api::{
    BotCheck, FieldDef, FieldValue, Form, FormId, FormList, Submission, SubmissionExport,
    SubmissionId, SubmissionList, SubmissionStatus,
};
use crate::store::{self, FormRow, SubmissionRow};
use crate::{export, fields};

const READ: Access = Access::Workspace(Scope::new("forms", Level::Read));
const WRITE: Access = Access::Workspace(Scope::new("forms", Level::Write));
const TAGS: &[&str] = &["forms"];
/// The default minimum fill time for `challenge` forms.
pub(crate) const DEFAULT_MIN_FILL_MS: u32 = 2000;
const MAX_MIN_FILL_MS: u32 = 600_000;
const MAX_NOTIFY: usize = 10;
const EXPORT_PAGE: i64 = 100;

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.scope_area("forms", "Forms and their submissions")?;
    cx.op(FormsList)?;
    cx.op(FormsCreate)?;
    cx.op(FormsGet)?;
    cx.op(FormsUpdate)?;
    cx.op(FormsDelete)?;
    cx.op(SubmissionsList)?;
    cx.op(SubmissionsGet)?;
    cx.op(SubmissionsUpdate)?;
    cx.op(SubmissionsDelete)?;
    cx.op(SubmissionsExport)?;
    Ok(())
}

// ---------------------------------------------------------------- conversions

/// The public URL sites post a form to.
pub(crate) fn endpoint(platform: &Platform, id: FormId) -> anyhow::Result<Url> {
    platform.base_url().join(&format!("/m/forms/public/{id}")).context("building the form endpoint")
}

pub(crate) fn decode_fields(row: &FormRow) -> anyhow::Result<Vec<FieldDef>> {
    serde_json::from_str(&row.fields)
        .with_context(|| format!("decoding the fields of form {}", row.id))
}

pub(crate) fn decode_notify(row: &FormRow) -> anyhow::Result<Vec<String>> {
    serde_json::from_str(&row.notify_emails)
        .with_context(|| format!("decoding the notify emails of form {}", row.id))
}

pub(crate) fn bot_check(row: &FormRow) -> BotCheck {
    if row.bot_check == BotCheck::Challenge.as_str() {
        BotCheck::Challenge
    } else {
        BotCheck::Honeypot
    }
}

pub(crate) fn min_fill_ms(row: &FormRow) -> u32 {
    u32::try_from(row.min_fill_ms).unwrap_or(DEFAULT_MIN_FILL_MS)
}

fn to_api(platform: &Platform, row: FormRow, inbox_count: i64) -> anyhow::Result<Form> {
    let endpoint = endpoint(platform, row.id)?;
    let challenge_url =
        Url::parse(&format!("{endpoint}/challenge")).context("building the challenge URL")?;
    Ok(Form {
        id: row.id,
        project_id: row.project_id,
        fields: decode_fields(&row)?,
        notify_emails: decode_notify(&row)?,
        bot_check: bot_check(&row),
        min_fill_ms: min_fill_ms(&row),
        redirect_url: row.redirect_url.as_deref().and_then(|url| Url::parse(url).ok()),
        endpoint,
        challenge_url,
        inbox_count: u64::try_from(inbox_count).unwrap_or(0),
        name: row.name,
        enabled: row.enabled,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub(crate) fn submission_to_api(row: SubmissionRow) -> anyhow::Result<Submission> {
    let data: BTreeMap<String, FieldValue> = serde_json::from_str(&row.data)
        .with_context(|| format!("decoding submission {}", row.id))?;
    Ok(Submission {
        id: row.id,
        form_id: row.form_id,
        status: SubmissionStatus::parse(&row.status).unwrap_or(SubmissionStatus::Spam),
        spam_reason: row.spam_reason,
        data,
        user_agent: row.user_agent,
        referer: row.referer,
        created_at: row.created_at,
    })
}

/// The form, or 404 when it isn't in the workspace.
async fn form_in(
    cx: &OpCtx,
    workspace_id: WorkspaceId,
    form_id: FormId,
) -> Result<FormRow, ApiError> {
    store::find_form(cx.platform().db().read(), workspace_id, form_id)
        .await?
        .ok_or(ApiError::NotFound)
}

// ---------------------------------------------------------------- inputs

/// Which forms to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListForms {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// Only this project's forms.
    #[serde(default)]
    project_id: Option<ProjectId>,
}
impl Validate for ListForms {}

/// A new form.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateForm {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project it belongs to.
    project_id: ProjectId,
    /// The display name.
    name: String,
    /// The accepted fields. Default none, which keeps every field.
    #[serde(default)]
    fields: Option<Vec<FieldDef>>,
    /// The bot check. Default `honeypot`.
    #[serde(default)]
    bot_check: Option<BotCheck>,
    /// With `challenge`: the least ms between loading the challenge and submitting, 0-600000. Default 2000.
    #[serde(default)]
    min_fill_ms: Option<u32>,
    /// Where browsers go after a plain form post, http or https.
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    redirect_url: Option<Url>,
    /// Who gets mail for each new submission, at most 10.
    #[serde(default)]
    notify_emails: Option<Vec<String>>,
}

impl Validate for CreateForm {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        check_settings(
            &mut errors,
            self.fields.as_deref(),
            self.min_fill_ms,
            self.redirect_url.as_ref(),
            self.notify_emails.as_deref(),
        );
        errors.into_result()
    }
}

/// Changes to a form. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateForm {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The form.
    form_id: FormId,
    /// The display name.
    #[serde(default)]
    name: Option<String>,
    /// Disabled forms refuse submissions.
    #[serde(default)]
    enabled: Option<bool>,
    /// The accepted fields; empty keeps every field.
    #[serde(default)]
    fields: Option<Vec<FieldDef>>,
    /// The bot check.
    #[serde(default)]
    bot_check: Option<BotCheck>,
    /// With `challenge`: the least ms between loading the challenge and submitting, 0-600000.
    #[serde(default)]
    min_fill_ms: Option<u32>,
    /// Where browsers go after a plain form post; `null` removes it.
    #[serde(default, deserialize_with = "present")]
    #[expect(clippy::option_option, reason = "absent keeps the URL, null removes it")]
    #[schema(value_type = Option<String>)]
    redirect_url: Option<Option<Url>>,
    /// Who gets mail for each new submission, at most 10.
    #[serde(default)]
    notify_emails: Option<Vec<String>>,
}

/// Tells an explicit `null` (`Some(None)`) from an absent field (`None`).
#[expect(clippy::option_option, reason = "absent keeps the value, null removes it")]
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

impl Validate for UpdateForm {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(name) = &self.name {
            check_name(&mut errors, "name", name);
        }
        check_settings(
            &mut errors,
            self.fields.as_deref(),
            self.min_fill_ms,
            self.redirect_url.as_ref().and_then(Option::as_ref),
            self.notify_emails.as_deref(),
        );
        errors.into_result()
    }
}

fn check_settings(
    errors: &mut FieldErrors,
    defs: Option<&[FieldDef]>,
    min_fill_ms: Option<u32>,
    redirect_url: Option<&Url>,
    notify_emails: Option<&[String]>,
) {
    if let Some(defs) = defs {
        fields::check_defs(errors, "fields", defs);
    }
    if min_fill_ms.is_some_and(|ms| ms > MAX_MIN_FILL_MS) {
        errors.add("min_fill_ms", format!("must be at most {MAX_MIN_FILL_MS}"));
    }
    if redirect_url.is_some_and(|url| !matches!(url.scheme(), "http" | "https")) {
        errors.add("redirect_url", "must be an http or https URL");
    }
    if let Some(emails) = notify_emails {
        if emails.len() > MAX_NOTIFY {
            errors.add("notify_emails", format!("must have at most {MAX_NOTIFY} addresses"));
        }
        for (i, email) in emails.iter().enumerate() {
            if !is_plausible_email(&normalize_email(email)) {
                errors.add(format!("notify_emails[{i}]"), "must be an email address");
            }
        }
    }
}

fn encode_notify(emails: &[String]) -> anyhow::Result<String> {
    let mut normalized: Vec<String> = Vec::with_capacity(emails.len());
    for email in emails.iter().map(|e| normalize_email(e)) {
        if !normalized.contains(&email) {
            normalized.push(email);
        }
    }
    serde_json::to_string(&normalized).context("encoding notify emails")
}

/// A form of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InForm {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The form.
    form_id: FormId,
}
impl Validate for InForm {}

/// Which submissions to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListSubmissions {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The form.
    form_id: FormId,
    /// Inbox or spam. Default inbox.
    #[serde(default)]
    status: Option<SubmissionStatus>,
    /// Only submissions older than this one, for the next page.
    #[serde(default)]
    before: Option<SubmissionId>,
    /// At most this many, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}
impl Validate for ListSubmissions {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if self.limit.is_some_and(|limit| !(1..=200).contains(&limit)) {
            errors.add("limit", "must be between 1 and 200");
        }
        errors.into_result()
    }
}

/// A submission of a form.
#[expect(clippy::struct_field_names, reason = "path params are named like the path segments")]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InSubmission {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The form.
    form_id: FormId,
    /// The submission.
    submission_id: SubmissionId,
}
impl Validate for InSubmission {}

/// A submission's new status.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateSubmission {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The form.
    form_id: FormId,
    /// The submission.
    submission_id: SubmissionId,
    /// Inbox or spam.
    status: SubmissionStatus,
}
impl Validate for UpdateSubmission {}

/// Which submissions to export.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExportSubmissions {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The form.
    form_id: FormId,
    /// Inbox or spam. Default inbox.
    #[serde(default)]
    status: Option<SubmissionStatus>,
}
impl Validate for ExportSubmissions {}

// ---------------------------------------------------------------- forms

struct FormsList;
impl Operation for FormsList {
    type Input = ListForms;
    type Output = FormList;
    const ID: &'static str = "forms.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/forms";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List the forms of a workspace";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListForms) -> Result<FormList, ApiError> {
        let rows =
            store::list_forms(cx.platform().db().read(), input.workspace_id, input.project_id)
                .await?;
        let items = rows
            .into_iter()
            .map(|r| to_api(cx.platform(), r.form, r.inbox_count))
            .collect::<anyhow::Result<_>>()?;
        Ok(FormList { items })
    }
}

struct FormsCreate;
impl Operation for FormsCreate {
    type Input = CreateForm;
    type Output = Form;
    const ID: &'static str = "forms.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/forms";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Create a form in a project";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateForm) -> Result<Form, ApiError> {
        let now = cx.now();
        let row = FormRow {
            id: FormId::new(),
            workspace_id: input.workspace_id,
            project_id: input.project_id,
            name: input.name.trim().to_owned(),
            enabled: true,
            fields: serde_json::to_string(&input.fields.unwrap_or_default())
                .context("encoding fields")?,
            bot_check: input.bot_check.unwrap_or_default().as_str().to_owned(),
            min_fill_ms: i64::from(input.min_fill_ms.unwrap_or(DEFAULT_MIN_FILL_MS)),
            redirect_url: input.redirect_url.map(String::from),
            notify_emails: encode_notify(&input.notify_emails.unwrap_or_default())?,
            challenge_key: random_token(),
            created_at: now,
            updated_at: now,
        };
        let mut tx = cx.write().await?;
        if !bizkit_core::projects::exists(&mut *tx, input.workspace_id, input.project_id).await? {
            let mut errors = FieldErrors::default();
            errors.add("project_id", "is not a project of this workspace");
            return Err(errors.into());
        }
        store::insert_form(&mut *tx, &row).await?;
        tx.commit().await?;
        Ok(to_api(cx.platform(), row, 0)?)
    }
}

struct FormsGet;
impl Operation for FormsGet {
    type Input = InForm;
    type Output = Form;
    const ID: &'static str = "forms.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/forms/{form_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a form";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InForm) -> Result<Form, ApiError> {
        let row = form_in(&cx, input.workspace_id, input.form_id).await?;
        let count =
            store::inbox_count(cx.platform().db().read(), input.workspace_id, input.form_id)
                .await?;
        Ok(to_api(cx.platform(), row, count)?)
    }
}

struct FormsUpdate;
impl Operation for FormsUpdate {
    type Input = UpdateForm;
    type Output = Form;
    const ID: &'static str = "forms.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/forms/{form_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Change a form's settings";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateForm) -> Result<Form, ApiError> {
        let mut tx = cx.write().await?;
        let mut row = store::find_form(&mut *tx, input.workspace_id, input.form_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if let Some(name) = input.name {
            name.trim().clone_into(&mut row.name);
        }
        if let Some(enabled) = input.enabled {
            row.enabled = enabled;
        }
        if let Some(defs) = input.fields {
            row.fields = serde_json::to_string(&defs).context("encoding fields")?;
        }
        if let Some(check) = input.bot_check {
            check.as_str().clone_into(&mut row.bot_check);
        }
        if let Some(ms) = input.min_fill_ms {
            row.min_fill_ms = i64::from(ms);
        }
        if let Some(url) = input.redirect_url {
            row.redirect_url = url.map(String::from);
        }
        if let Some(emails) = input.notify_emails {
            row.notify_emails = encode_notify(&emails)?;
        }
        row.updated_at = cx.now();
        store::update_form(&mut *tx, &row).await?;
        let count = store::inbox_count(&mut *tx, input.workspace_id, input.form_id).await?;
        tx.commit().await?;
        Ok(to_api(cx.platform(), row, count)?)
    }
}

struct FormsDelete;
impl Operation for FormsDelete {
    type Input = InForm;
    type Output = NoContent;
    const ID: &'static str = "forms.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/forms/{form_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Delete a form and its submissions";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InForm) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let deleted = store::delete_form(&mut *tx, input.workspace_id, input.form_id).await?;
        tx.commit().await?;
        if deleted { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

// ---------------------------------------------------------------- submissions

struct SubmissionsList;
impl Operation for SubmissionsList {
    type Input = ListSubmissions;
    type Output = SubmissionList;
    const ID: &'static str = "forms.submissions.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/forms/{form_id}/submissions";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List a form's submissions, newest first";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListSubmissions) -> Result<SubmissionList, ApiError> {
        form_in(&cx, input.workspace_id, input.form_id).await?;
        let limit = input.limit.unwrap_or(50);
        let status = input.status.unwrap_or_default();
        let rows = store::list_submissions(
            cx.platform().db().read(),
            input.workspace_id,
            input.form_id,
            store::Page {
                status: status.as_str(),
                before: input.before,
                limit: i64::from(limit) + 1,
            },
        )
        .await?;
        let mut items =
            rows.into_iter().map(submission_to_api).collect::<anyhow::Result<Vec<_>>>()?;
        let more = items.len() > usize::try_from(limit).unwrap_or(usize::MAX);
        items.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let next_before = if more { items.last().map(|s| s.id) } else { None };
        Ok(SubmissionList { items, next_before })
    }
}

struct SubmissionsGet;
impl Operation for SubmissionsGet {
    type Input = InSubmission;
    type Output = Submission;
    const ID: &'static str = "forms.submissions.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/forms/{form_id}/submissions/{submission_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a submission";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InSubmission) -> Result<Submission, ApiError> {
        let row = store::find_submission(
            cx.platform().db().read(),
            input.workspace_id,
            input.form_id,
            input.submission_id,
        )
        .await?
        .ok_or(ApiError::NotFound)?;
        Ok(submission_to_api(row)?)
    }
}

struct SubmissionsUpdate;
impl Operation for SubmissionsUpdate {
    type Input = UpdateSubmission;
    type Output = Submission;
    const ID: &'static str = "forms.submissions.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/forms/{form_id}/submissions/{submission_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Move a submission to the inbox or to spam";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateSubmission) -> Result<Submission, ApiError> {
        let mut tx = cx.write().await?;
        let (ws, form, id) = (input.workspace_id, input.form_id, input.submission_id);
        if !store::set_status(&mut *tx, ws, form, id, input.status.as_str()).await? {
            return Err(ApiError::NotFound);
        }
        let row =
            store::find_submission(&mut *tx, ws, form, id).await?.ok_or(ApiError::NotFound)?;
        tx.commit().await?;
        Ok(submission_to_api(row)?)
    }
}

struct SubmissionsDelete;
impl Operation for SubmissionsDelete {
    type Input = InSubmission;
    type Output = NoContent;
    const ID: &'static str = "forms.submissions.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/forms/{form_id}/submissions/{submission_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Delete a submission";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InSubmission) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let deleted = store::delete_submission(
            &mut *tx,
            input.workspace_id,
            input.form_id,
            input.submission_id,
        )
        .await?;
        tx.commit().await?;
        if deleted { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

struct SubmissionsExport;
impl Operation for SubmissionsExport {
    type Input = ExportSubmissions;
    type Output = SubmissionExport;
    const ID: &'static str = "forms.submissions.export";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/forms/{form_id}/export";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Export a form's submissions as CSV, newest first, at most 10000";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(
        &self,
        cx: OpCtx,
        input: ExportSubmissions,
    ) -> Result<SubmissionExport, ApiError> {
        let form = form_in(&cx, input.workspace_id, input.form_id).await?;
        let status = input.status.unwrap_or_default();
        // Pages through rows and stops at the row or byte budget, so a form
        // full of 64 KiB spam never sits in memory all at once.
        let mut submissions = Vec::new();
        let (mut bytes, mut before, mut more_rows) = (0, None, false);
        loop {
            let page = store::Page { status: status.as_str(), before, limit: EXPORT_PAGE };
            let rows = store::list_submissions(
                cx.platform().db().read(),
                input.workspace_id,
                input.form_id,
                page,
            )
            .await?;
            let last_page = rows.len() < usize::try_from(EXPORT_PAGE).unwrap_or(usize::MAX);
            for row in rows {
                if submissions.len() == export::MAX_ROWS || bytes > export::MAX_BYTES {
                    more_rows = true;
                    break;
                }
                bytes += row.data.len();
                before = Some(row.id);
                submissions.push(submission_to_api(row)?);
            }
            if more_rows || last_page {
                break;
            }
        }
        let (csv, cut) = export::csv(&decode_fields(&form)?, &submissions);
        let date =
            cx.now().to_rfc3339().map_or_else(|_| String::new(), |t| t.chars().take(10).collect());
        Ok(SubmissionExport {
            filename: format!("{}-{}-{date}.csv", slug(&form.name), status.as_str()),
            csv,
            truncated: more_rows || cut,
        })
    }
}

/// A file-name-safe form of `name`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() { "form".to_owned() } else { trimmed.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_for_file_names() {
        assert_eq!(slug("Contact us!"), "contact-us", "punctuation");
        assert_eq!(slug("   "), "form", "empty");
        assert_eq!(slug("Ünïcode Form"), "n-code-form", "non-ascii");
    }
}
