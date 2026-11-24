//! The signing operations: create, list, read, edit, send, void and delete
//! signature requests, and download their PDFs.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context as _;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bizkit_core::auth::{hash_token, is_plausible_email, normalize_email, random_token};
use bizkit_core::db::WriteTx;
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
use crate::fields::{self, Field, FieldInput, FieldKind, FieldValue};
use crate::pdf;
use crate::store::{self, FieldRow, NewEvent, RequestRow, SignerRow};

pub(crate) const READ_SCOPE: Scope = Scope::new("signing", Level::Read);
pub(crate) const WRITE_SCOPE: Scope = Scope::new("signing", Level::Write);
const READ: Access = Access::Workspace(READ_SCOPE);
const WRITE: Access = Access::Workspace(WRITE_SCOPE);
const ADMIN: Access = Access::Workspace(Scope::new("signing", Level::Admin));
const TAGS: &[&str] = &["signing"];
/// The largest PDF `pdf_base64` takes. The API's body limit is 1 MiB, and
/// base64 adds a third. Larger files go through the upload route.
pub(crate) const MAX_PDF_BYTES: usize = 700 * 1024;
/// The largest file `signing.requests.document` returns as base64 JSON;
/// a 25 MiB upload would be about 34 MiB of JSON.
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SIGNERS: usize = 10;
const MAX_ORDER: u8 = 10;
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
    cx.op(SignersPut)?;
    cx.op(FieldsPut)?;
    cx.op(RequestsSend)?;
    cx.op(RequestsVoid)?;
    cx.op(RequestsDelete)?;
    cx.op(RequestsRetry)?;
    Ok(())
}

pub(crate) fn invalid(field: &str, message: &str) -> ApiError {
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
        page_count: row.page_count.and_then(|count| u32::try_from(count).ok()),
        signers: signers
            .into_iter()
            .map(|signer| Signer {
                signed: signer.status == "signed",
                id: signer.id,
                name: signer.name,
                email: signer.email,
                signing_order: u8::try_from(signer.signing_order).unwrap_or(1),
                signed_at: signer.signed_at,
            })
            .collect(),
        created_at: row.created_at,
        sent_at: row.sent_at,
        completed_at: row.completed_at,
        voided_at: row.voided_at,
    })
}

fn to_field(row: FieldRow) -> anyhow::Result<Field> {
    let value = row
        .value
        .as_deref()
        .map(serde_json::from_str::<FieldValue>)
        .transpose()
        .context("reading a field value")?;
    Ok(Field {
        id: row.id,
        signer_id: row.signer_id,
        kind: FieldKind::parse(&row.kind)?,
        page: u32::try_from(row.page).context("reading a field's page")?,
        x: row.x,
        y: row.y,
        width: row.width,
        height: row.height,
        required: row.required,
        label: row.label,
        value,
        filled_at: row.filled_at,
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

async fn detail(platform: &Platform, row: RequestRow) -> Result<SigningRequestDetail, ApiError> {
    let db = platform.db().read();
    let events = store::events(db, row.workspace_id, row.id)
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
    let fields = store::fields(db, row.workspace_id, row.id)
        .await?
        .into_iter()
        .map(to_field)
        .collect::<anyhow::Result<_>>()
        .map_err(internal)?;
    Ok(SigningRequestDetail { request: load(platform, row).await?, fields, events })
}

fn require_draft(row: &RequestRow) -> Result<(), ApiError> {
    if row.status == RequestStatus::Draft.as_str() {
        Ok(())
    } else {
        Err(ApiError::Conflict("only a draft can be changed".to_owned()))
    }
}

// ---------------------------------------------------------------- inputs

const fn first_group() -> u8 {
    1
}

/// A person to ask for a signature.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewSigner {
    /// Their name, printed under their signature.
    name: String,
    /// Where their link goes.
    email: String,
    /// Their group, 1 (default) to 10. Groups get their links in order,
    /// each once everyone in the groups before it signed; equal numbers
    /// sign in parallel, and numbers need not be consecutive.
    #[serde(default = "first_group")]
    signing_order: u8,
}

/// Checks a signer list: 1 to 10 signers with names, different emails and
/// groups 1 to 10.
fn check_signers<'a>(
    errors: &mut FieldErrors,
    signers: impl ExactSizeIterator<Item = (&'a str, &'a str, u8)>,
) {
    if !(1..=MAX_SIGNERS).contains(&signers.len()) {
        errors.add("signers", format!("must have 1 to {MAX_SIGNERS} signers"));
    }
    let mut seen = BTreeSet::new();
    for (at, (name, email, order)) in signers.enumerate() {
        check_name(errors, &format!("signers[{at}].name"), name);
        let email = normalize_email(email);
        if !is_plausible_email(&email) {
            errors.add(format!("signers[{at}].email"), "must be an email address");
        } else if !seen.insert(email) {
            errors.add(format!("signers[{at}].email"), "is already a signer");
        }
        if !(1..=MAX_ORDER).contains(&order) {
            errors.add(format!("signers[{at}].signing_order"), "must be 1 to 10");
        }
    }
}

/// What a new draft says, apart from its file: the JSON `meta` part of an
/// upload, and the same fields of `signing.requests.create`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftMeta {
    /// The project it belongs to.
    pub(crate) project_id: ProjectId,
    /// What signers see as the subject; 1-120 characters.
    pub(crate) title: String,
    /// A note for signers, at most 2,000 characters.
    #[serde(default)]
    pub(crate) message: String,
    /// 1 to 10 signers with different emails, in the order they print.
    pub(crate) signers: Vec<NewSigner>,
}

impl DraftMeta {
    fn check(&self, errors: &mut FieldErrors) {
        check_name(errors, "title", &self.title);
        if self.message.chars().count() > MAX_MESSAGE {
            errors.add("message", format!("must be at most {MAX_MESSAGE} characters"));
        }
        check_signers(
            errors,
            self.signers.iter().map(|s| (s.name.as_str(), s.email.as_str(), s.signing_order)),
        );
    }
}

fn check_file_name(errors: &mut FieldErrors, field: &str, file_name: &str) {
    let name_len = file_name.trim().chars().count();
    if name_len == 0 || name_len > MAX_FILE_NAME {
        errors.add(field, format!("must be 1 to {MAX_FILE_NAME} characters"));
    }
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
    /// The PDF, standard base64, at most 700 KiB decoded and 500 pages.
    /// Encrypted PDFs are refused. Larger files go through the upload
    /// route, `POST /m/signing/upload/workspaces/{workspace_id}/requests`.
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
        check_file_name(&mut errors, "file_name", &self.file_name);
        if self.pdf_base64.len() > MAX_PDF_BYTES.div_ceil(3) * 4 {
            errors.add("pdf_base64", "must be at most 700 KiB decoded");
        }
        check_signers(
            &mut errors,
            self.signers.iter().map(|s| (s.name.as_str(), s.email.as_str(), s.signing_order)),
        );
        errors.into_result()
    }
}

/// Where a new draft's PDF came from, which sets the field its errors name
/// and how long it may take to parse.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PdfSource {
    /// `pdf_base64` in the JSON body of `signing.requests.create`.
    Base64,
    /// The `file` part of a multipart upload.
    Upload,
}

impl PdfSource {
    const fn field(self) -> &'static str {
        match self {
            Self::Base64 => "pdf_base64",
            Self::Upload => "file",
        }
    }

    const fn timeout(self) -> std::time::Duration {
        match self {
            Self::Base64 => pdf::BASE64_PARSE_TIMEOUT,
            Self::Upload => pdf::UPLOAD_PARSE_TIMEOUT,
        }
    }
}

/// A new draft's PDF.
pub(crate) struct DraftFile {
    pub(crate) name: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) source: PdfSource,
}

/// Checks a draft and its PDF and stores it, with its signers and a
/// `created` event. The JSON operation and the upload route both create
/// drafts here.
pub(crate) async fn create_draft(
    platform: &Platform,
    workspace_id: WorkspaceId,
    principal: Option<&Principal>,
    meta: DraftMeta,
    file: DraftFile,
) -> Result<SigningRequest, ApiError> {
    let mut errors = FieldErrors::default();
    meta.check(&mut errors);
    let name_field = match file.source {
        PdfSource::Base64 => "file_name",
        PdfSource::Upload => "file",
    };
    check_file_name(&mut errors, name_field, &file.name);
    errors.into_result()?;
    let page_count = match pdf::check(file.bytes.clone(), file.source.timeout()).await {
        Ok(pages) => pages,
        Err(problem) => return Err(invalid(file.source.field(), problem)),
    };
    if !bizkit_core::projects::exists(platform.db().read(), workspace_id, meta.project_id).await? {
        return Err(ApiError::NotFound);
    }
    let now = platform.now();
    let (created_by, sender) = match principal {
        Some(Principal::User(user)) => (Some(user.user_id), user.email.clone()),
        Some(Principal::ApiKey(key)) => (None, format!("API key {}", key.key_id)),
        None => (None, "unknown".to_owned()),
    };
    let row = RequestRow {
        id: SigningRequestId::new(),
        workspace_id,
        project_id: meta.project_id,
        title: meta.title.trim().to_owned(),
        message: meta.message.trim().to_owned(),
        status: RequestStatus::Draft.as_str().to_owned(),
        file_name: file.name.trim().to_owned(),
        original_sha256: sha256_hex(&file.bytes),
        signed_sha256: None,
        created_by,
        sender,
        failure: None,
        created_at: now,
        updated_at: now,
        sent_at: None,
        completed_at: None,
        voided_at: None,
        page_count: Some(i64::from(page_count)),
    };
    let signers: Vec<SignerRow> = meta
        .signers
        .into_iter()
        .zip(0_i64..)
        .map(|(signer, position)| SignerRow {
            id: SignerId::new(),
            workspace_id: row.workspace_id,
            request_id: row.id,
            position,
            signing_order: i64::from(signer.signing_order),
            name: signer.name.trim().to_owned(),
            email: normalize_email(&signer.email),
            status: "pending".to_owned(),
            signature: None,
            signed_at: None,
        })
        .collect();
    let mut tx = platform.db().write().await?;
    store::insert_request(&mut *tx, &row).await?;
    store::put_file(&mut *tx, &row, DocumentVersion::Original, &file.bytes).await?;
    for signer in &signers {
        store::insert_signer(&mut *tx, signer).await?;
    }
    store::insert_event(&mut *tx, NewEvent::of(&row, EventKind::Created, now)).await?;
    tx.commit().await?;
    to_api(row, signers)
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

/// A signer of a draft: an existing one by `id`, or a new one without.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignerSlot {
    /// An existing signer of the request, who keeps their fields. Leave it
    /// out for a new signer.
    #[serde(default)]
    id: Option<SignerId>,
    /// Their name, printed under their signature.
    name: String,
    /// Where their link goes.
    email: String,
    /// Their group, 1 (default) to 10.
    #[serde(default = "first_group")]
    signing_order: u8,
}

/// A draft's whole signer list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PutSigners {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The request.
    request_id: SigningRequestId,
    /// 1 to 10 signers in the order they print. Signers of the request
    /// left out are removed with their fields.
    signers: Vec<SignerSlot>,
}

impl Validate for PutSigners {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_signers(
            &mut errors,
            self.signers.iter().map(|s| (s.name.as_str(), s.email.as_str(), s.signing_order)),
        );
        let mut ids = BTreeSet::new();
        for (at, signer) in self.signers.iter().enumerate() {
            if let Some(id) = signer.id
                && !ids.insert(id)
            {
                errors.add(format!("signers[{at}].id"), "is listed twice");
            }
        }
        errors.into_result()
    }
}

/// A draft's whole field list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PutFields {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The request.
    request_id: SigningRequestId,
    /// At most 200 fields, replacing the ones the draft has.
    fields: Vec<FieldInput>,
}

impl Validate for PutFields {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        fields::check(&self.fields, &mut errors);
        errors.into_result()
    }
}

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
        let bytes = STANDARD
            .decode(input.pdf_base64.trim())
            .map_err(|_| invalid("pdf_base64", "must be standard base64"))?;
        if bytes.len() > MAX_PDF_BYTES {
            return Err(invalid("pdf_base64", "must be at most 700 KiB decoded"));
        }
        let meta = DraftMeta {
            project_id: input.project_id,
            title: input.title,
            message: input.message,
            signers: input.signers,
        };
        let file = DraftFile { name: input.file_name, bytes, source: PdfSource::Base64 };
        create_draft(cx.platform(), input.workspace_id, cx.principal(), meta, file).await
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
    const SUMMARY: &'static str = "Get a signature request with its fields and audit trail";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InRequest) -> Result<SigningRequestDetail, ApiError> {
        let platform = cx.platform();
        detail(platform, find(platform, input.workspace_id, input.request_id).await?).await
    }
}

/// The authenticated route that serves a request's PDF as a file.
fn file_route(row: &RequestRow, version: DocumentVersion) -> String {
    format!(
        "GET /m/signing/workspaces/{}/requests/{}/{}.pdf",
        row.workspace_id,
        row.id,
        version.as_str()
    )
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
    /// 413 when the file is over 8 MiB; the message names the file route
    /// that serves it.
    const ERRORS: &'static [StatusCode] = &[StatusCode::PAYLOAD_TOO_LARGE];

    async fn call(&self, cx: OpCtx, input: InDocument) -> Result<Document, ApiError> {
        let platform = cx.platform();
        let db = platform.db().read();
        let row = find(platform, input.workspace_id, input.request_id).await?;
        let len = store::file_len(db, &row, input.version).await?.ok_or(ApiError::NotFound)?;
        if usize::try_from(len).map_or(true, |len| len > MAX_DOCUMENT_BYTES) {
            return Err(ApiError::PayloadTooLarge(format!(
                "the file is over 8 MiB, too large for base64 JSON; download it with {}",
                file_route(&row, input.version)
            )));
        }
        let bytes = store::get_file(db, &row, input.version).await?.ok_or(ApiError::NotFound)?;
        let file_name = match input.version {
            DocumentVersion::Original => row.file_name.clone(),
            DocumentVersion::Signed => pdf::signed_file_name(&row.file_name),
        };
        Ok(Document { file_name, sha256: sha256_hex(&bytes), pdf_base64: STANDARD.encode(&bytes) })
    }
}

struct SignersPut;
impl Operation for SignersPut {
    type Input = PutSigners;
    type Output = SigningRequestDetail;
    const ID: &'static str = "signing.requests.signers.put";
    const METHOD: Method = Method::Put;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests/{request_id}/signers";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Replace a draft's signers; listed IDs keep their fields";
    const TAGS: &'static [&'static str] = TAGS;
    /// 409 when the request isn't a draft.
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: PutSigners) -> Result<SigningRequestDetail, ApiError> {
        let platform = cx.platform();
        let mut tx = cx.write().await?;
        let row = store::find_request(&mut *tx, input.workspace_id, input.request_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        require_draft(&row)?;
        let mut existing: BTreeMap<SignerId, SignerRow> =
            store::signers(&mut *tx, row.workspace_id, row.id)
                .await?
                .into_iter()
                .map(|signer| (signer.id, signer))
                .collect();
        let mut errors = FieldErrors::default();
        let mut kept = Vec::new();
        let mut added = Vec::new();
        for (slot, position) in input.signers.into_iter().zip(0_i64..) {
            let name = slot.name.trim().to_owned();
            let email = normalize_email(&slot.email);
            let signing_order = i64::from(slot.signing_order);
            match slot.id {
                Some(id) => match existing.remove(&id) {
                    Some(signer) => {
                        kept.push(SignerRow { position, signing_order, name, email, ..signer });
                    }
                    None => errors
                        .add(format!("signers[{position}].id"), "is not a signer of this request"),
                },
                None => added.push(SignerRow {
                    id: SignerId::new(),
                    workspace_id: row.workspace_id,
                    request_id: row.id,
                    position,
                    signing_order,
                    name,
                    email,
                    status: "pending".to_owned(),
                    signature: None,
                    signed_at: None,
                }),
            }
        }
        errors.into_result()?;
        for dropped in existing.values() {
            store::delete_signer(&mut *tx, dropped).await?;
        }
        // UNIQUE (request_id, position) holds after each row's update, so
        // kept signers step aside to negative positions before they move.
        for (signer, aside) in kept.iter().zip(1_i64..) {
            let parked = SignerRow { position: -aside, ..signer.clone() };
            store::update_signer(&mut *tx, &parked).await?;
        }
        for signer in &kept {
            store::update_signer(&mut *tx, signer).await?;
        }
        for signer in &added {
            store::insert_signer(&mut *tx, signer).await?;
        }
        tx.commit().await?;
        detail(platform, find(platform, input.workspace_id, input.request_id).await?).await
    }
}

/// The page count of a request from before fields existed, read from its
/// file. `None` when the request already has one or isn't a draft.
async fn missing_page_count(
    platform: &Platform,
    row: &RequestRow,
) -> Result<Option<i64>, ApiError> {
    if row.page_count.is_some() || row.status != RequestStatus::Draft.as_str() {
        return Ok(None);
    }
    let bytes = store::get_file(platform.db().read(), row, DocumentVersion::Original)
        .await?
        .ok_or_else(|| internal(anyhow::anyhow!("request {} has no original file", row.id)))?;
    let pages = pdf::check(bytes, pdf::UPLOAD_PARSE_TIMEOUT).await.map_err(|problem| {
        internal(anyhow::anyhow!("reading the page count of request {}: {problem}", row.id))
    })?;
    Ok(Some(i64::from(pages)))
}

struct FieldsPut;
impl Operation for FieldsPut {
    type Input = PutFields;
    type Output = SigningRequestDetail;
    const ID: &'static str = "signing.requests.fields.put";
    const METHOD: Method = Method::Put;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests/{request_id}/fields";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Replace the fields on a draft's pages";
    const TAGS: &'static [&'static str] = TAGS;
    /// 409 when the request isn't a draft.
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: PutFields) -> Result<SigningRequestDetail, ApiError> {
        let platform = cx.platform();
        let before = find(platform, input.workspace_id, input.request_id).await?;
        // Parsing can take seconds, so it runs before the write transaction.
        let read_count = missing_page_count(platform, &before).await?;
        let mut tx = cx.write().await?;
        let row = store::find_request(&mut *tx, input.workspace_id, input.request_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        require_draft(&row)?;
        let page_count = match (row.page_count, read_count) {
            (Some(count), _) => count,
            (None, Some(count)) => {
                store::set_page_count(&mut *tx, &row, count).await?;
                count
            }
            (None, None) => {
                return Err(internal(anyhow::anyhow!("request {} has no page count", row.id)));
            }
        };
        let signers: BTreeSet<SignerId> = store::signers(&mut *tx, row.workspace_id, row.id)
            .await?
            .into_iter()
            .map(|signer| signer.id)
            .collect();
        let mut errors = FieldErrors::default();
        for (at, field) in input.fields.iter().enumerate() {
            if !signers.contains(&field.signer_id) {
                errors.add(format!("fields[{at}].signer_id"), "is not a signer of this request");
            }
            if i64::from(field.page) > page_count {
                errors.add(format!("fields[{at}].page"), format!("must be at most {page_count}"));
            }
        }
        errors.into_result()?;
        store::delete_fields(&mut *tx, &row).await?;
        for (field, position) in input.fields.into_iter().zip(0_i64..) {
            let field = FieldRow {
                id: fields::FieldId::new(),
                workspace_id: row.workspace_id,
                request_id: row.id,
                signer_id: field.signer_id,
                position,
                kind: field.kind.as_str().to_owned(),
                page: i64::from(field.page),
                x: field.x,
                y: field.y,
                width: field.width,
                height: field.height,
                required: field.required,
                label: field.label.trim().to_owned(),
                value: None,
                filled_at: None,
            };
            store::insert_field(&mut *tx, &field).await?;
        }
        tx.commit().await?;
        detail(platform, find(platform, input.workspace_id, input.request_id).await?).await
    }
}

/// The link a signer opens.
pub(crate) fn sign_link(platform: &Platform, token: &str) -> anyhow::Result<url::Url> {
    let mut url =
        platform.base_url().join("/m/signing/public/sign").context("building the sign link")?;
    url.query_pairs_mut().append_pair("t", token);
    Ok(url)
}

/// Gives each of `signers` a link, queues its mail and records a `sent`
/// event, in the caller's transaction.
pub(crate) async fn mail_group(
    platform: &Platform,
    tx: &mut WriteTx,
    row: &RequestRow,
    signers: &[SignerRow],
    now: UnixMillis,
) -> anyhow::Result<()> {
    for signer in signers {
        let token = random_token();
        store::set_token(&mut **tx, signer, &hash_token(&token)).await?;
        let link = sign_link(platform, &token)?;
        let mail = crate::mail::sign_request(row, signer, &link);
        platform.bus().enqueue(tx, &SendMail(mail), Enqueue::default()).await?;
        let detail = format!("Link mailed to {}.", signer.email);
        let event = NewEvent {
            signer_id: Some(signer.id),
            detail: Some(&detail),
            ..NewEvent::of(row, EventKind::Sent, now)
        };
        store::insert_event(&mut **tx, event).await?;
    }
    Ok(())
}

struct RequestsSend;
impl Operation for RequestsSend {
    type Input = InRequest;
    type Output = SigningRequest;
    const ID: &'static str = "signing.requests.send";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/signing/requests/{request_id}/send";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Mail the first signing-order group their links";
    const TAGS: &'static [&'static str] = TAGS;
    /// 409 when the request isn't a draft or a signer has no signature
    /// field; 429 past 100 signer mails per workspace per hour.
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT, StatusCode::TOO_MANY_REQUESTS];

    async fn call(&self, cx: OpCtx, input: InRequest) -> Result<SigningRequest, ApiError> {
        let platform = cx.platform();
        let now = cx.now();
        let mut tx = cx.write().await?;
        let row = store::find_request(&mut *tx, input.workspace_id, input.request_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if row.status != RequestStatus::Draft.as_str() {
            return Err(ApiError::Conflict("only a draft can be sent".to_owned()));
        }
        let unplaced = store::signers_without_signature_field(&mut *tx, &row).await?;
        if !unplaced.is_empty() {
            return Err(ApiError::Conflict(format!(
                "every signer needs a signature field; add one for {}",
                unplaced.join(", ")
            )));
        }
        // Later groups mail without a second check, so the cap counts every
        // signer of a request at its first send, and a request that passed
        // it never stalls halfway.
        let signers = store::signers(&mut *tx, row.workspace_id, row.id).await?;
        let hour_ago = UnixMillis(now.0 - 3_600_000);
        let recent = store::signers_sent_since(&mut *tx, row.workspace_id, hour_ago).await?;
        if recent + i64::try_from(signers.len()).unwrap_or(i64::MAX) > MAILS_PER_HOUR {
            return Err(ApiError::RateLimited { retry_after_secs: 3600 });
        }
        if !store::set_status(&mut *tx, &row, &[RequestStatus::Draft], RequestStatus::Sent, now)
            .await?
        {
            return Err(ApiError::Conflict("only a draft can be sent".to_owned()));
        }
        let group = store::next_group(&mut *tx, row.workspace_id, row.id).await?;
        mail_group(platform, &mut tx, &row, &group, now).await?;
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
