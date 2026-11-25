//! Raw routes for files too large for base64 JSON: the multipart create
//! route, nested at `/m/signing/upload`, and the PDF downloads, nested at
//! `/m/signing`. Both check access like a workspace operation.

use axum::extract::multipart::{Field, MultipartError, MultipartRejection};
use axum::extract::{Multipart, Path};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use bizkit_core::error::ErrorBody;
use bizkit_core::ops::{Scope, authorize_workspace};
use bizkit_core::upload::read_file;
use bizkit_core::validate::FieldErrors;
use bizkit_core::{ApiError, Authn, Platform};
use http::StatusCode;
use utoipa::{OpenApi, ToSchema};

use crate::api::{DocumentVersion, SigningRequest, SigningRequestId};
use crate::ops::{self, DraftFile, DraftMeta, PdfSource, READ_SCOPE, WRITE_SCOPE};
use crate::pdf;
use crate::public::{disposition_name, pdf_response};
use crate::store;

/// The `meta` part is a few signers and a note.
const MAX_META_BYTES: usize = 64 * 1024;

/// The upload router.
pub(crate) fn upload_router() -> Router {
    Router::new().route("/workspaces/{workspace_id}/requests", post(create))
}

/// The authenticated file router.
pub(crate) fn file_router() -> Router {
    Router::new()
        .route(
            "/workspaces/{workspace_id}/requests/{request_id}/original.pdf",
            get(|platform, authn, path| file(platform, authn, path, DocumentVersion::Original)),
        )
        .route(
            "/workspaces/{workspace_id}/requests/{request_id}/signed.pdf",
            get(|platform, authn, path| file(platform, authn, path, DocumentVersion::Signed)),
        )
}

/// A multipart create: the draft's details and its PDF.
#[derive(ToSchema)]
#[expect(dead_code, reason = "exists for its utoipa schema")]
struct UploadForm {
    /// The draft as JSON: `project_id`, `title`, `message` and `signers`,
    /// as in `signing.requests.create`.
    meta: DraftMeta,
    /// The PDF, at most 25 MiB and 500 pages. Its part's file name becomes
    /// the request's `file_name`.
    #[schema(value_type = String, format = Binary)]
    file: Vec<u8>,
}

#[utoipa::path(post, path = "/workspaces/{workspace_id}/requests",
    operation_id = "signing.upload.create", tag = "signing",
    summary = "Upload a PDF of up to 25 MiB and name its signers, as a draft",
    params(("workspace_id" = String, Path, description = "The workspace")),
    request_body(content = UploadForm, content_type = "multipart/form-data",
        description = "Parts `meta` (JSON) and `file` (the PDF). Needs `signing:write`."),
    responses(
        (status = 201, description = "The draft", body = SigningRequest),
        (status = 400, description = "The multipart body is malformed", body = ErrorBody),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 403, description = "Cross-site, or missing `signing:write`", body = ErrorBody),
        (status = 404, description = "No such workspace or project", body = ErrorBody),
        (status = 408, description = "The upload took too long"),
        (status = 413, description = "The body or the file is too large", content(
            (String = "text/plain"),
            (ErrorBody = "application/json"),
        )),
        (status = 422, description = "The meta or the file needs fixing", body = ErrorBody),
    ))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn create_docs() {}

#[derive(OpenApi)]
#[openapi(paths(create_docs))]
pub(crate) struct UploadDocs;

#[utoipa::path(get, path = "/workspaces/{workspace_id}/requests/{request_id}/original.pdf",
    operation_id = "signing.files.original", tag = "signing",
    summary = "A request's PDF as uploaded. Needs `signing:read`.",
    params(("workspace_id" = String, Path, description = "The workspace"),
           ("request_id" = String, Path, description = "The request")),
    responses(
        (status = 200, description = "The PDF", content_type = "application/pdf"),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 403, description = "Missing `signing:read`", body = ErrorBody),
        (status = 404, description = "No such request", body = ErrorBody),
    ))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn original_docs() {}

#[utoipa::path(get, path = "/workspaces/{workspace_id}/requests/{request_id}/signed.pdf",
    operation_id = "signing.files.signed", tag = "signing",
    summary = "A request's sealed PDF, once completed. Needs `signing:read`.",
    params(("workspace_id" = String, Path, description = "The workspace"),
           ("request_id" = String, Path, description = "The request")),
    responses(
        (status = 200, description = "The PDF", content_type = "application/pdf"),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 403, description = "Missing `signing:read`", body = ErrorBody),
        (status = 404, description = "No such request, or not completed", body = ErrorBody),
    ))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn signed_docs() {}

#[derive(OpenApi)]
#[openapi(paths(original_docs, signed_docs))]
pub(crate) struct FileDocs;

async fn authorize(
    platform: &Platform,
    authn: &Authn,
    workspace_id: &str,
    scope: &Scope,
) -> Result<bizkit_core::ids::WorkspaceId, ApiError> {
    let access =
        authorize_workspace(platform, authn.principal.as_ref(), workspace_id, scope).await?;
    Ok(access.workspace_id)
}

async fn create(
    Extension(platform): Extension<Platform>,
    Extension(authn): Extension<Authn>,
    Path(workspace_id): Path<String>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Response {
    match create_inner(&platform, &authn, &workspace_id, multipart).await {
        Ok(request) => (StatusCode::CREATED, Json(request)).into_response(),
        Err(err) => err.into_response(),
    }
}

async fn create_inner(
    platform: &Platform,
    authn: &Authn,
    workspace_id: &str,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<SigningRequest, ApiError> {
    let workspace_id = authorize(platform, authn, workspace_id, &WRITE_SCOPE).await?;
    let mut multipart =
        multipart.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let mut meta = None;
    let mut file = None;
    let mut seen = Vec::new();
    let mut errors = FieldErrors::default();
    while let Some(field) = multipart.next_field().await.map_err(|err| multipart_error(&err))? {
        let name = field.name().unwrap_or_default().to_owned();
        if matches!(name.as_str(), "meta" | "file") && seen.contains(&name) {
            errors.add(name, "must be given once");
            continue;
        }
        seen.push(name.clone());
        match name.as_str() {
            "meta" => match read_meta(field).await? {
                Some(bytes) => match serde_json::from_slice::<DraftMeta>(&bytes) {
                    Ok(parsed) => meta = Some(parsed),
                    Err(err) => errors.add("meta", format!("must be the draft as JSON: {err}")),
                },
                None => errors.add("meta", "must be at most 64 KiB"),
            },
            "file" => {
                let file_name = field.file_name().unwrap_or_default().to_owned();
                file = Some((file_name, read_file(field).await?));
            }
            "" => errors.add("parts", "a part has no name; send only `meta` and `file`"),
            other => errors.add("parts", format!("`{other}` is not a part of this upload")),
        }
    }
    if meta.is_none() && !errors.as_map().contains_key("meta") {
        errors.add("meta", "is required");
    }
    if file.is_none() {
        errors.add("file", "is required");
    }
    errors.into_result()?;
    let (Some(meta), Some((name, bytes))) = (meta, file) else {
        return Err(ApiError::BadRequest("the upload is missing a part".to_owned()));
    };
    let draft_file = DraftFile { name, bytes: bytes.to_vec(), source: PdfSource::Upload };
    ops::create_draft(platform, workspace_id, authn.principal.as_ref(), meta, draft_file).await
}

/// The `meta` part, or `None` once it passes [`MAX_META_BYTES`]. It stops
/// reading there, so a large part never sits in memory.
async fn read_meta(mut field: Field<'_>) -> Result<Option<Vec<u8>>, ApiError> {
    let mut meta = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|err| multipart_error(&err))? {
        if meta.len() + chunk.len() > MAX_META_BYTES {
            return Ok(None);
        }
        meta.extend_from_slice(&chunk);
    }
    Ok(Some(meta))
}

fn multipart_error(err: &MultipartError) -> ApiError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::PayloadTooLarge(err.body_text())
    } else {
        ApiError::BadRequest(err.body_text())
    }
}

async fn file(
    Extension(platform): Extension<Platform>,
    Extension(authn): Extension<Authn>,
    Path((workspace_id, request_id)): Path<(String, String)>,
    version: DocumentVersion,
) -> Response {
    let found = async {
        let workspace_id = authorize(&platform, &authn, &workspace_id, &READ_SCOPE).await?;
        let request_id: SigningRequestId = request_id.parse().map_err(|_| ApiError::NotFound)?;
        let db = platform.db().read();
        let row =
            store::find_request(db, workspace_id, request_id).await?.ok_or(ApiError::NotFound)?;
        let bytes = store::get_file(db, &row, version).await?.ok_or(ApiError::NotFound)?;
        Ok::<_, ApiError>((bytes, row.file_name))
    }
    .await;
    match found {
        Ok((bytes, name)) => {
            let (disposition, name) = match version {
                DocumentVersion::Original => ("inline", name),
                DocumentVersion::Signed => ("attachment", pdf::signed_file_name(&name)),
            };
            pdf_response(bytes, &format!("{disposition}; filename=\"{}\"", disposition_name(&name)))
        }
        Err(err) => err.into_response(),
    }
}
