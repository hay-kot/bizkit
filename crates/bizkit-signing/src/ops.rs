//! The signing operations: create, list, read, send, void and delete
//! signature requests, and download their PDFs.

use anyhow::Context as _;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bizkit_core::auth::{hash_token, is_plausible_email, normalize_email, random_token};
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::jobs::{Enqueue, SendMail};
use bizkit_core::ops::{
    Access, Level, Method, NoContent, OpCtx, Operation, Principal, Scope, Success,
};
use bizkit_core::time::UnixMillis;
use bizkit_core::validate::{FieldErrors, Validate, check_name};
use bizkit_core::{ApiError, ModuleContext, Platform};
use http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::api::{
    AuditEvent, Document, DocumentVersion, EventKind, RequestStatus, Signer, SignerId,
    SigningRequest, SigningRequestDetail, SigningRequestId, SigningRequestList,
};
use crate::complete::Complete;
use crate::pdf;
use crate::store::{self, NewEvent, RequestRow, SignerRow};

const READ: Access = Access::Workspace(Scope::new("signing", Level::Read));
const WRITE: Access = Access::Workspace(Scope::new("signing", Level::Write));
const ADMIN: Access = Access::Workspace(Scope::new("signing", Level::Admin));
const TAGS: &[&str] = &["signing"];
/// The largest PDF a request takes. The API's body limit is 1 MiB, and
/// base64 adds a third.
pub(crate) const MAX_PDF_BYTES: usize = 700 * 1024;
const MAX_SIGNERS: usize = 10;
const MAX_MESSAGE: usize = 2000;
const MAX_FILE_NAME: usize = 200;
/// Signer mails a workspace may queue per hour, so the instance's domain
/// can't mail strangers in bulk.
const MAILS_PER_HOUR: i64 = 100;

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.scope_area("signing", "Signature requests and their PDFs")?;
    cx.op(RequestsCreate)?;
    cx.op(RequestsList)?;
    cx.op(RequestsGet)?;
    cx.op(RequestsDocument)?;
    cx.op(RequestsSend)?;
    cx.op(RequestsVoid)?;
    cx.op(RequestsDelete)?;
    cx.op(RequestsRetry)?;
    Ok(())
}

fn invalid(field: &str, message: &str) -> ApiError {
    let mut errors = FieldErrors::default();
    errors.add(field, message);
    ApiError::Validation(errors)
}

fn internal(err: impl Into<anyhow::Error>) -> ApiError {
    ApiError::Internal(err.into())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
        // Writing to a String can't fail.
        let _ignored = write!(out, "{byte:02x}");
        out
    })
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    hex(&sha2::Sha256::digest(bytes))
}

pub(crate) fn to_api(row: RequestRow, signers: Vec<SignerRow>) -> Result<SigningRequest, ApiError> {
    Ok(SigningRequest {
        id: row.id,
        project_id: row.project_id,
        title: row.title,
        message: row.message,
        status: RequestStatus::parse(&row.status).map_err(internal)?,
        failure: row.failure,
        sender: row.sender,
        file_name: row.file_name,
        original_sha256: row.original_sha256,
        signed_sha256: row.signed_sha256,
        signers: signers
            .into_iter()
            .map(|signer| Signer {
                signed: signer.status == "signed",
                id: signer.id,
                name: signer.name,
                email: signer.email,
                signed_at: signer.signed_at,
            })
            .collect(),
        created_at: row.created_at,
        sent_at: row.sent_at,
        completed_at: row.completed_at,
        voided_at: row.voided_at,
    })
}

async fn find(
    platform: &Platform,
    workspace_id: WorkspaceId,
    id: SigningRequestId,
) -> Result<RequestRow, ApiError> {
    store::find_request(platform.db().read(), workspace_id, id).await?.ok_or(ApiError::NotFound)
}

async fn load(platform: &Platform, row: RequestRow) -> Result<SigningRequest, ApiError> {
    let signers = store::signers(platform.db().read(), row.workspace_id, row.id).await?;
    to_api(row, signers)
}

// ---------------------------------------------------------------- inputs

/// A person to ask for a signature.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewSigner {
    /// Their name, printed under their signature.
    name: String,
    /// Where their link goes.
    email: String,
}

/// A new signature request, as a draft.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateRequest {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project it belongs to.
    project_id: ProjectId,
    /// What signers see as the subject; 1-120 characters.
    title: String,
    /// A note for signers, at most 2,000 characters.
    #[serde(default)]
    message: String,
    /// The file's name, such as `contract.pdf`.
    file_name: String,
    /// The PDF, standard base64, at most 700 KiB decoded. Encrypted PDFs are refused.
    pdf_base64: String,
    /// 1 to 10 signers with different emails, in the order they print.
    signers: Vec<NewSigner>,
}

impl Validate for CreateRequest {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "title", &self.title);
        if self.message.chars().count() > MAX_MESSAGE {
            errors.add("message", format!("must be at most {MAX_MESSAGE} characters"));
        }
        let name_len = self.file_name.trim().chars().count();
        if name_len == 0 || name_len > MAX_FILE_NAME {
            errors.add("file_name", format!("must be 1 to {MAX_FILE_NAME} characters"));
        }
        if self.pdf_base64.len() > MAX_PDF_BYTES.div_ceil(3) * 4 {
            errors.add("pdf_base64", "must be at most 700 KiB decoded");
        }
        if !(1..=MAX_SIGNERS).contains(&self.signers.len()) {
            errors.add("signers", format!("must have 1 to {MAX_SIGNERS} signers"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for (at, signer) in self.signers.iter().enumerate() {
            check_name(&mut errors, &format!("signers[{at}].name"), &signer.name);
            let email = normalize_email(&signer.email);
            if !is_plausible_email(&email) {
                errors.add(format!("signers[{at}].email"), "must be an email address");
            } else if !seen.insert(email) {
                errors.add(format!("signers[{at}].email"), "is already a signer");
            }
        }
        errors.into_result()
    }
}

/// A project's requests.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListRequests {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
}
impl Validate for ListRequests {}

/// One request.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InRequest {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The request.
    request_id: SigningRequestId,
}
impl Validate for InRequest {}

/// One PDF of a request.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InDocument {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The request.
    request_id: SigningRequestId,
    /// `original` (default) or `signed`, once completed.
    #[serde(default)]
    version: DocumentVersion,
}
impl Validate for InDocument {}

// ---------------------------------------------------------------- operations

struct RequestsCreate;
impl Operation for RequestsCreate {
    type Input = CreateRequest;
    type Output = SigningRequest;
    const ID: &'static str = "signing.requests.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Upload a PDF and name its signers, as a draft";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateRequest) -> Result<SigningRequest, ApiError> {
        let platform = cx.platform();
        let bytes = STANDARD
            .decode(input.pdf_base64.trim())
            .map_err(|_| invalid("pdf_base64", "must be standard base64"))?;
        if bytes.len() > MAX_PDF_BYTES {
            return Err(invalid("pdf_base64", "must be at most 700 KiB decoded"));
        }
        if let Err(problem) = pdf::check(bytes.clone()).await {
            return Err(invalid("pdf_base64", problem));
        }
        if !bizkit_core::projects::exists(
            platform.db().read(),
            input.workspace_id,
            input.project_id,
        )
        .await?
        {
            return Err(ApiError::NotFound);
        }
        let now = cx.now();
        let (created_by, sender) = match cx.principal() {
            Some(Principal::User(user)) => (Some(user.user_id), user.email.clone()),
            Some(Principal::ApiKey(key)) => (None, format!("API key {}", key.key_id)),
            None => (None, "unknown".to_owned()),
        };
        let row = RequestRow {
            id: SigningRequestId::new(),
            workspace_id: input.workspace_id,
            project_id: input.project_id,
            title: input.title.trim().to_owned(),
            message: input.message.trim().to_owned(),
            status: RequestStatus::Draft.as_str().to_owned(),
            file_name: input.file_name.trim().to_owned(),
            original_sha256: sha256_hex(&bytes),
            signed_sha256: None,
            created_by,
            sender,
            failure: None,
            created_at: now,
            updated_at: now,
            sent_at: None,
            completed_at: None,
            voided_at: None,
        };
        let signers: Vec<SignerRow> = input
            .signers
            .into_iter()
            .zip(0_i64..)
            .map(|(signer, position)| SignerRow {
                id: SignerId::new(),
                workspace_id: row.workspace_id,
                request_id: row.id,
                position,
                name: signer.name.trim().to_owned(),
                email: normalize_email(&signer.email),
                status: "pending".to_owned(),
                signature: None,
                signed_at: None,
            })
            .collect();
        let mut tx = cx.write().await?;
        store::insert_request(&mut *tx, &row).await?;
        store::put_file(&mut *tx, &row, DocumentVersion::Original, &bytes).await?;
        for signer in &signers {
            store::insert_signer(&mut *tx, signer).await?;
        }
        store::insert_event(&mut *tx, NewEvent::of(&row, EventKind::Created, now)).await?;
        tx.commit().await?;
        to_api(row, signers)
    }
}

struct RequestsList;
impl Operation for RequestsList {
    type Input = ListRequests;
    type Output = SigningRequestList;
    const ID: &'static str = "signing.requests.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List a project's signature requests";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListRequests) -> Result<SigningRequestList, ApiError> {
        let db = cx.platform().db().read();
        let rows = store::list_requests(db, input.workspace_id, input.project_id).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let signers = store::signers(db, row.workspace_id, row.id).await?;
            items.push(to_api(row, signers)?);
        }
        Ok(SigningRequestList { items })
    }
}

struct RequestsGet;
impl Operation for RequestsGet {
    type Input = InRequest;
    type Output = SigningRequestDetail;
    const ID: &'static str = "signing.requests.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests/{request_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Get a signature request with its audit trail";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InRequest) -> Result<SigningRequestDetail, ApiError> {
        let platform = cx.platform();
        let row = find(platform, input.workspace_id, input.request_id).await?;
        let events = store::events(platform.db().read(), row.workspace_id, row.id)
            .await?
            .into_iter()
            .map(|event| {
                Ok(AuditEvent {
                    kind: EventKind::parse(&event.kind)?,
                    at: event.at,
                    signer_id: event.signer_id,
                    ip: event.ip,
                    user_agent: event.user_agent,
                    detail: event.detail,
                })
            })
            .collect::<anyhow::Result<_>>()
            .map_err(internal)?;
        Ok(SigningRequestDetail { request: load(platform, row).await?, events })
    }
}

struct RequestsDocument;
impl Operation for RequestsDocument {
    type Input = InDocument;
    type Output = Document;
    const ID: &'static str = "signing.requests.document";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests/{request_id}/document";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Download a request's original or sealed PDF";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InDocument) -> Result<Document, ApiError> {
        let platform = cx.platform();
        let row = find(platform, input.workspace_id, input.request_id).await?;
        let bytes = store::get_file(platform.db().read(), &row, input.version)
            .await?
            .ok_or(ApiError::NotFound)?;
        let file_name = match input.version {
            DocumentVersion::Original => row.file_name.clone(),
            DocumentVersion::Signed => pdf::signed_file_name(&row.file_name),
        };
        Ok(Document { file_name, sha256: sha256_hex(&bytes), pdf_base64: STANDARD.encode(&bytes) })
    }
}

/// The link a signer opens.
pub(crate) fn sign_link(platform: &Platform, token: &str) -> anyhow::Result<url::Url> {
    let mut url =
        platform.base_url().join("/m/signing/public/sign").context("building the sign link")?;
    url.query_pairs_mut().append_pair("t", token);
    Ok(url)
}

struct RequestsSend;
impl Operation for RequestsSend {
    type Input = InRequest;
    type Output = SigningRequest;
    const ID: &'static str = "signing.requests.send";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests/{request_id}/send";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Mail each signer their link";
    const TAGS: &'static [&'static str] = TAGS;
    /// 409 when the request isn't a draft; 429 past 100 signer mails per
    /// workspace per hour.
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT, StatusCode::TOO_MANY_REQUESTS];

    async fn call(&self, cx: OpCtx, input: InRequest) -> Result<SigningRequest, ApiError> {
        let platform = cx.platform();
        let now = cx.now();
        let mut tx = cx.write().await?;
        let row = store::find_request(&mut *tx, input.workspace_id, input.request_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if !store::set_status(&mut *tx, &row, &[RequestStatus::Draft], RequestStatus::Sent, now)
            .await?
        {
            return Err(ApiError::Conflict("only a draft can be sent".to_owned()));
        }
        let signers = store::signers(&mut *tx, row.workspace_id, row.id).await?;
        let hour_ago = UnixMillis(now.0 - 3_600_000);
        let recent = store::mails_since(&mut *tx, row.workspace_id, hour_ago).await?;
        if recent + i64::try_from(signers.len()).unwrap_or(i64::MAX) > MAILS_PER_HOUR {
            return Err(ApiError::RateLimited { retry_after_secs: 3600 });
        }
        for signer in &signers {
            let token = random_token();
            store::set_token(&mut *tx, signer, &hash_token(&token)).await?;
            let link = sign_link(platform, &token).map_err(internal)?;
            let mail = crate::mail::sign_request(&row, signer, &link);
            platform.bus().enqueue(&mut tx, &SendMail(mail), Enqueue::default()).await?;
            let detail = format!("Link mailed to {}.", signer.email);
            let event = NewEvent {
                signer_id: Some(signer.id),
                detail: Some(&detail),
                ..NewEvent::of(&row, EventKind::Sent, now)
            };
            store::insert_event(&mut *tx, event).await?;
        }
        tx.commit().await?;
        load(platform, find(platform, input.workspace_id, input.request_id).await?).await
    }
}

struct RequestsVoid;
impl Operation for RequestsVoid {
    type Input = InRequest;
    type Output = SigningRequest;
    const ID: &'static str = "signing.requests.void";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests/{request_id}/void";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Cancel a request; its links stop working";
    const TAGS: &'static [&'static str] = TAGS;
    /// 409 when the request is completed, voided, or signed by everyone.
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: InRequest) -> Result<SigningRequest, ApiError> {
        let platform = cx.platform();
        let now = cx.now();
        let mut tx = cx.write().await?;
        let row = store::find_request(&mut *tx, input.workspace_id, input.request_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if store::pending_signers(&mut *tx, &row).await? == 0 {
            return Err(ApiError::Conflict("everyone has signed; it can't be voided".to_owned()));
        }
        let from = [RequestStatus::Draft, RequestStatus::Sent];
        if !store::set_status(&mut *tx, &row, &from, RequestStatus::Voided, now).await? {
            return Err(ApiError::Conflict(
                "only a draft or sent request can be voided".to_owned(),
            ));
        }
        store::insert_event(&mut *tx, NewEvent::of(&row, EventKind::Voided, now)).await?;
        tx.commit().await?;
        load(platform, find(platform, input.workspace_id, input.request_id).await?).await
    }
}

struct RequestsDelete;
impl Operation for RequestsDelete {
    type Input = InRequest;
    type Output = NoContent;
    const ID: &'static str = "signing.requests.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests/{request_id}";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Delete a draft or voided request";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;
    /// 409 when the request was sent or completed: signers hold its record.
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: InRequest) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let row = store::find_request(&mut *tx, input.workspace_id, input.request_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        let status = RequestStatus::parse(&row.status).map_err(internal)?;
        if !matches!(status, RequestStatus::Draft | RequestStatus::Voided) {
            return Err(ApiError::Conflict(
                "only a draft or voided request can be deleted".to_owned(),
            ));
        }
        store::delete_request(&mut *tx, &row).await?;
        tx.commit().await?;
        Ok(NoContent)
    }
}

struct RequestsRetry;
impl Operation for RequestsRetry {
    type Input = InRequest;
    type Output = SigningRequest;
    const ID: &'static str = "signing.requests.retry";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests/{request_id}/retry";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Try sealing a failed request again";
    const TAGS: &'static [&'static str] = TAGS;
    /// 409 when the request isn't `failed`.
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: InRequest) -> Result<SigningRequest, ApiError> {
        let platform = cx.platform();
        let now = cx.now();
        let mut tx = cx.write().await?;
        let row = store::find_request(&mut *tx, input.workspace_id, input.request_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if !store::set_status(&mut *tx, &row, &[RequestStatus::Failed], RequestStatus::Sent, now)
            .await?
        {
            return Err(ApiError::Conflict("only a failed request can be retried".to_owned()));
        }
        let job = Complete { workspace_id: row.workspace_id, request_id: row.id };
        platform.bus().enqueue(&mut tx, &job, Enqueue::default()).await?;
        tx.commit().await?;
        load(platform, find(platform, input.workspace_id, input.request_id).await?).await
    }
}
