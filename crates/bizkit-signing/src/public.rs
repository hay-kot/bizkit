//! The signer API, nested at `/m/signing/public`, which the web app's
//! signer view (`/sign?t=`) calls. It sits outside authentication: the
//! link's token is the credential, and the database keeps only its hash.
//! GETs change nothing, so mail scanners that follow a link leave no trace;
//! the view reports a view, and a sign POST fills every field at once.
//! Every response is `no-store` and `no-referrer`, because the token is in
//! the request.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context as _;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use bizkit_core::auth::hash_token;
use bizkit_core::error::ErrorBody;
use bizkit_core::jobs::Enqueue;
use bizkit_core::net::ClientIp;
use bizkit_core::time::UnixMillis;
use bizkit_core::validate::{FieldErrors, from_json};
use bizkit_core::{ApiError, Platform};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::api::{DocumentVersion, EventKind, RequestStatus};
use crate::complete::Complete;
use crate::fields::{Field, FieldId, FieldKind, FieldValue};
use crate::pdf;
use crate::signature::Signature;
use crate::store::{self, NewEvent, RequestRow, SignerRow};

/// What a signer agrees to; recorded with each signature.
pub(crate) const CONSENT: &str = "I agree to sign this document electronically, and that my electronic \
                                  signature has the same effect as a handwritten one.";
/// Sign posts are small: two signatures' strokes and a few lines of text.
const MAX_BODY: usize = 256 * 1024;
const MAX_USER_AGENT: usize = 300;
const MAX_TEXT: usize = 200;
const JSON_CSP: &str = "default-src 'none'; frame-ancestors 'none'";

/// The public router.
pub(crate) fn router() -> Router {
    Router::new()
        .route("/session", get(session))
        .route("/sign", get(sign_redirect).post(sign))
        .route("/viewed", post(viewed))
        .route("/document", get(document))
        .route("/signed.pdf", get(signed_pdf))
        .layer(DefaultBodyLimit::max(MAX_BODY))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        session_docs,
        sign_redirect_docs,
        sign_docs,
        viewed_docs,
        document_docs,
        signed_pdf_docs
    ),
    components(schemas(SignerSession, SignInput, SignResult, TokenBody, ErrorBody))
)]
pub(crate) struct PublicDocs;

#[utoipa::path(get, path = "/session", operation_id = "signing.public.session", tag = "signing",
    summary = "What a signer's link shows: the request, the signer and their fields",
    description = "Changes nothing. A signer who signed still gets their session, with their \
        fields' values; once the request completes, `signed.pdf` with the same token downloads it.",
    params(("t" = String, Query, description = "The signer's link token")),
    responses((status = 200, description = "The session", body = SignerSession),
              (status = 404, description = "The link no longer works: unknown, voided, or not out for \
                signature", body = ErrorBody)))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn session_docs() {}

#[utoipa::path(get, path = "/sign", operation_id = "signing.public.sign_redirect", tag = "signing",
    summary = "Old sign links: go to the web app's signer view",
    description = "Links mailed before the signer view moved to the web app point here.",
    params(("t" = String, Query, description = "The signer's link token")),
    responses((status = 303, description = "To `/sign?t=` in the web app")))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn sign_redirect_docs() {}

#[utoipa::path(post, path = "/sign", operation_id = "signing.public.sign", tag = "signing",
    summary = "Adopt a signature, fill the signer's fields and sign",
    description = "Fills every field of the signer at once. Signature and initials fields take the \
        adopted `signature` and `initials`; date fields take today's UTC date; `values` fill text and \
        checkbox fields. Every required field must be filled, and a required checkbox ticked.",
    request_body(content = SignInput, content_type = "application/json"),
    responses((status = 200, description = "Signed", body = SignResult),
              (status = 400, description = "The body isn't JSON", body = ErrorBody),
              (status = 404, description = "The link no longer works, or the signer signed already",
                body = ErrorBody),
              (status = 413, description = "The body is over 256 KiB"),
              (status = 422, description = "Fields need fixing; `fields` names them", body = ErrorBody)))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn sign_docs() {}

#[utoipa::path(post, path = "/viewed", operation_id = "signing.public.viewed", tag = "signing",
    summary = "Record that a signer opened the signer view",
    request_body(content = TokenBody, content_type = "application/json"),
    responses((status = 204, description = "Recorded, or the link no longer works"),
              (status = 400, description = "The body isn't JSON", body = ErrorBody),
              (status = 422, description = "The body has no token", body = ErrorBody)))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn viewed_docs() {}

#[utoipa::path(get, path = "/document", operation_id = "signing.public.document", tag = "signing",
    summary = "The PDF to sign, for a signer's link",
    params(("t" = String, Query, description = "The signer's link token")),
    responses((status = 200, description = "The PDF", content_type = "application/pdf"),
              (status = 404, description = "The link no longer works")))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn document_docs() {}

#[utoipa::path(get, path = "/signed.pdf", operation_id = "signing.public.signed_pdf", tag = "signing",
    summary = "The sealed PDF, once everyone signed",
    params(("t" = String, Query, description = "The signer's link token")),
    responses((status = 200, description = "The PDF", content_type = "application/pdf"),
              (status = 404, description = "Not completed, or the link no longer works")))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn signed_pdf_docs() {}

/// Where a signer downloads the sealed PDF.
pub(crate) fn signed_link(platform: &Platform, token: &str) -> anyhow::Result<url::Url> {
    let mut url = platform
        .base_url()
        .join("/m/signing/public/signed.pdf")
        .context("building the download link")?;
    url.query_pairs_mut().append_pair("t", token);
    Ok(url)
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    #[serde(default)]
    t: String,
}

/// A body that carries only the link token.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SigningToken)]
struct TokenBody {
    /// The signer's link token.
    t: String,
}

/// The request, as its signer sees it.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = SigningSessionRequest)]
struct SessionRequest {
    /// What signers see as the subject.
    title: String,
    /// The sender's note.
    message: String,
    /// The PDF's file name.
    file_name: String,
    /// Where the request is.
    status: RequestStatus,
    /// Pages in the PDF. Empty only on requests made before fields existed.
    page_count: Option<u32>,
}

/// Whether a signer signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SigningSignerStatus)]
enum SignerStatus {
    /// Yet to sign.
    Pending,
    /// Signed.
    Signed,
}

/// The signer the link belongs to.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = SigningSessionSigner)]
struct SessionSigner {
    /// Their name, as the sender gave it.
    name: String,
    /// Their address.
    email: String,
    /// Whether they signed.
    status: SignerStatus,
}

/// What a signer's link shows.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = SigningSignerSession)]
struct SignerSession {
    /// The request.
    request: SessionRequest,
    /// The signer.
    signer: SessionSigner,
    /// This signer's fields only, in request order, with values once signed.
    /// Empty for requests made before fields: the signer then adopts a
    /// signature only.
    fields: Vec<Field>,
    /// The text the signer agrees to by signing.
    consent: String,
}

/// A signer's fill of one text or checkbox field.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SigningFieldFill)]
struct FieldFill {
    /// The field.
    field_id: FieldId,
    /// For a text field: one line of at most 200 Latin characters. Empty
    /// text leaves an optional field empty.
    #[serde(default)]
    text: Option<String>,
    /// For a checkbox field: whether it's ticked.
    #[serde(default)]
    checked: Option<bool>,
}

/// A signer's signature, initials and fills.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SigningSignInput)]
struct SignInput {
    /// The signer's link token.
    t: String,
    /// Must be true: the signer agrees to `consent` from the session.
    consent: bool,
    /// The adopted signature, for every signature field.
    signature: Option<Signature>,
    /// The adopted initials, for every initials field. Required when the
    /// signer has one, and refused when they have none.
    #[serde(default)]
    initials: Option<Signature>,
    /// Fills of text and checkbox fields; each field at most once.
    #[serde(default)]
    values: Vec<FieldFill>,
}

/// The request after a signature.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = SigningSignResult)]
struct SignResult {
    /// Where the request is. It stays `sent` until the seal job completes
    /// it after the last signature.
    status: RequestStatus,
}

/// A signer's link, resolved: their row and their request.
struct Link {
    signer: SignerRow,
    request: RequestRow,
}

/// Finds the signer by their token's hash, an unguessable key, then reads
/// the workspace from the row.
async fn resolve(platform: &Platform, token: &str) -> anyhow::Result<Option<Link>> {
    if token.is_empty() || token.len() > 100 {
        return Ok(None);
    }
    let db = platform.db().read();
    let Some(signer) = store::signer_by_token(db, &hash_token(token)).await? else {
        return Ok(None);
    };
    let request = store::find_request(db, signer.workspace_id, signer.request_id).await?;
    Ok(request.map(|request| Link { signer, request }))
}

fn secure(response: &mut Response, csp: &str) {
    let headers = response.headers_mut();
    if let Ok(csp) = HeaderValue::from_str(csp) {
        headers.insert(header::CONTENT_SECURITY_POLICY, csp);
    }
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
}

fn json_response<T: Serialize>(result: Result<T, ApiError>) -> Response {
    let mut response = match result {
        Ok(body) => (StatusCode::OK, Json(body)).into_response(),
        Err(err) => err.into_response(),
    };
    secure(&mut response, JSON_CSP);
    response
}

fn parse<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
    let value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|_| ApiError::BadRequest("the body must be JSON".to_owned()))?;
    Ok(from_json(value)?)
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|agent| agent.chars().take(MAX_USER_AGENT).collect())
}

/// The signer's fields, read from the API's own conversion.
async fn signer_fields(platform: &Platform, signer: &SignerRow) -> anyhow::Result<Vec<Field>> {
    store::signer_fields(platform.db().read(), signer)
        .await?
        .into_iter()
        .map(crate::ops::to_field)
        .collect()
}

async fn session(
    Extension(platform): Extension<Platform>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let result = async {
        let link = resolve(&platform, &query.t).await?.ok_or(ApiError::NotFound)?;
        let request = &link.request;
        let signed = link.signer.status == "signed";
        if request.status == RequestStatus::Voided.as_str()
            || (!signed && request.status != RequestStatus::Sent.as_str())
        {
            return Err(ApiError::NotFound);
        }
        let fields = signer_fields(&platform, &link.signer).await?;
        Ok(SignerSession {
            request: SessionRequest {
                title: request.title.clone(),
                message: request.message.clone(),
                file_name: request.file_name.clone(),
                status: RequestStatus::parse(&request.status)?,
                page_count: request.page_count.and_then(|count| u32::try_from(count).ok()),
            },
            signer: SessionSigner {
                name: link.signer.name,
                email: link.signer.email,
                status: if signed { SignerStatus::Signed } else { SignerStatus::Pending },
            },
            fields,
            consent: CONSENT.to_owned(),
        })
    }
    .await;
    json_response(result)
}

/// Links mailed before the signer view moved into the web app.
async fn sign_redirect(
    Extension(platform): Extension<Platform>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let mut response = match crate::ops::sign_link(&platform, &query.t) {
        Ok(url) => match HeaderValue::from_str(url.as_str()) {
            Ok(location) => (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response(),
            Err(err) => ApiError::Internal(err.into()).into_response(),
        },
        Err(err) => ApiError::Internal(err).into_response(),
    };
    secure(&mut response, JSON_CSP);
    response
}

async fn viewed(
    Extension(platform): Extension<Platform>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let input: TokenBody = match parse(&body) {
        Ok(input) => input,
        Err(err) => return json_response::<()>(Err(err)),
    };
    let recorded = async {
        let Some(link) = resolve(&platform, &input.t).await? else { return Ok(()) };
        if link.request.status != RequestStatus::Sent.as_str() || link.signer.status == "signed" {
            return Ok(());
        }
        let mut tx = platform.db().write().await?;
        if !store::has_viewed(&mut *tx, &link.signer).await? {
            let ip = ip.map(|ip| ip.to_string());
            let agent = user_agent(&headers);
            let event = NewEvent {
                signer_id: Some(link.signer.id),
                ip: ip.as_deref(),
                user_agent: agent.as_deref(),
                ..NewEvent::of(&link.request, EventKind::Viewed, platform.now())
            };
            store::insert_event(&mut *tx, event).await?;
            tx.commit().await?;
        }
        anyhow::Ok(())
    }
    .await;
    if let Err(err) = recorded {
        tracing::warn!(error = format!("{err:#}"), "recording a view failed");
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    secure(&mut response, JSON_CSP);
    response
}

/// What a sign POST stores.
#[derive(Debug, PartialEq)]
struct Fill {
    signature: Signature,
    initials: Option<Signature>,
    values: Vec<(FieldId, FieldValue)>,
}

/// One line of 1 to 200 characters the standard font prints.
fn check_text(text: &str) -> Result<(), &'static str> {
    if text.chars().count() > MAX_TEXT {
        return Err("must be at most 200 characters");
    }
    if text.chars().any(char::is_control) {
        return Err("must be one line");
    }
    if !text.chars().all(|c| pdf::win_ansi(c).is_some()) {
        return Err("must use Latin letters");
    }
    Ok(())
}

/// The text and checkbox fills a signer gave, checked against their
/// `fields`.
#[derive(Default)]
struct Given {
    values: BTreeMap<FieldId, FieldValue>,
    /// Fields whose fill has an error already, so a missing value isn't
    /// reported on top of it.
    bad: BTreeSet<FieldId>,
}

fn given_values(fills: Vec<FieldFill>, fields: &[Field], errors: &mut FieldErrors) -> Given {
    let by_id: BTreeMap<FieldId, &Field> = fields.iter().map(|field| (field.id, field)).collect();
    let mut seen = BTreeSet::new();
    let mut given = Given::default();
    for (at, fill) in fills.into_iter().enumerate() {
        let path = |name: &str| format!("values[{at}].{name}");
        let field = by_id
            .get(&fill.field_id)
            .filter(|field| matches!(field.kind, FieldKind::Text | FieldKind::Checkbox));
        let Some(field) = field else {
            errors.add(path("field_id"), "must be one of your text or checkbox fields");
            continue;
        };
        if !seen.insert(fill.field_id) {
            errors.add(path("field_id"), "must be listed once");
            continue;
        }
        let mut problem = |name: &str, message: &str| {
            errors.add(path(name), message);
            given.bad.insert(field.id);
        };
        if field.kind == FieldKind::Checkbox {
            if fill.text.is_some() {
                problem("text", "must be left out for a checkbox");
            }
            match fill.checked {
                Some(checked) => {
                    given.values.insert(field.id, FieldValue::Checkbox { checked });
                }
                None => problem("checked", "must be given"),
            }
        } else {
            if fill.checked.is_some() {
                problem("checked", "must be left out for a text field");
            }
            let text = fill.text.as_deref().map_or("", str::trim);
            if let Err(message) = check_text(text) {
                problem("text", message);
            } else if !text.is_empty() {
                given.values.insert(field.id, FieldValue::Text { text: text.to_owned() });
            }
        }
    }
    given
}

/// Checks a sign POST against the signer's `fields` and works out every
/// value to store; date fields get `today`.
fn fill(input: SignInput, fields: &[Field], today: &str) -> Result<Fill, FieldErrors> {
    let mut errors = FieldErrors::default();
    if !input.consent {
        errors.add("consent", "must be true to sign electronically");
    }
    let mut adopt = |name: &str, given: Option<Signature>, needed: bool| match given {
        None if needed => {
            errors.add(name, "must be given");
            None
        }
        None => None,
        Some(_) if !needed => {
            errors.add(name, "must be left out: you have no fields for it");
            None
        }
        Some(signature) => signature.checked().map_err(|problem| errors.add(name, problem)).ok(),
    };
    // Every signer has a signature field once a request is sent; a signer
    // from before fields has none and still signs.
    let signature = adopt("signature", input.signature, true);
    let needs_initials = fields.iter().any(|field| field.kind == FieldKind::Initials);
    let initials = adopt("initials", input.initials, needs_initials);
    let mut given = given_values(input.values, fields, &mut errors);

    let mut values = Vec::new();
    for field in fields {
        let value = match field.kind {
            FieldKind::Signature => Some(FieldValue::Signed),
            FieldKind::Initials => Some(FieldValue::Initialed),
            FieldKind::Date => Some(FieldValue::Date { date: today.to_owned() }),
            FieldKind::Text | FieldKind::Checkbox => given.values.remove(&field.id),
        };
        let filled = match &value {
            Some(FieldValue::Checkbox { checked }) => *checked,
            other => other.is_some(),
        };
        if field.required && !filled && !given.bad.contains(&field.id) {
            let problem =
                if field.kind == FieldKind::Checkbox { "must be ticked" } else { "must be filled" };
            errors.add(format!("fields.{}", field.id), problem);
        }
        if let Some(value) = value {
            values.push((field.id, value));
        }
    }
    match signature {
        Some(signature) if errors.is_empty() => Ok(Fill { signature, initials, values }),
        _ => Err(errors),
    }
}

/// The UTC date of `at`, `YYYY-MM-DD`.
fn utc_date(at: UnixMillis) -> anyhow::Result<String> {
    let date = at.to_datetime().context("the clock is out of range")?.date();
    Ok(format!("{:04}-{:02}-{:02}", date.year(), u8::from(date.month()), date.day()))
}

async fn sign(
    Extension(platform): Extension<Platform>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let input: SignInput = parse(&body)?;
        let link = resolve(&platform, &input.t).await?.ok_or(ApiError::NotFound)?;
        if link.request.status != RequestStatus::Sent.as_str() || link.signer.status == "signed" {
            return Err(ApiError::NotFound);
        }
        let now = platform.now();
        let fields = signer_fields(&platform, &link.signer).await?;
        let fill = fill(input, &fields, &utc_date(now)?)?;
        let client = Client { ip: ip.map(|ip| ip.to_string()), user_agent: user_agent(&headers) };
        if !record_signature(&platform, &link, &fill, &client, now).await? {
            return Err(ApiError::NotFound);
        }
        Ok(SignResult { status: RequestStatus::Sent })
    }
    .await;
    json_response(result)
}

struct Client {
    ip: Option<String>,
    user_agent: Option<String>,
}

/// Stores the signature, the field values and the audit row, then mails
/// the next signing-order group once this one is done, or queues the seal
/// after the last signature, in one transaction. False when the request
/// or signer changed since the session loaded (voided, or signed in
/// another tab).
async fn record_signature(
    platform: &Platform,
    link: &Link,
    fill: &Fill,
    client: &Client,
    now: UnixMillis,
) -> anyhow::Result<bool> {
    let signature = serde_json::to_string(&fill.signature).context("encoding a signature")?;
    let initials = fill
        .initials
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("encoding initials")?;
    let mut tx = platform.db().write().await?;
    let current = store::find_request(&mut *tx, link.request.workspace_id, link.request.id).await?;
    if current.is_none_or(|row| row.status != RequestStatus::Sent.as_str()) {
        return Ok(false);
    }
    if !store::mark_signed(&mut *tx, &link.signer, &signature, initials.as_deref(), now).await? {
        return Ok(false);
    }
    for (field_id, value) in &fill.values {
        let value = serde_json::to_string(value).context("encoding a field value")?;
        store::fill_field(&mut *tx, &link.signer, *field_id, &value, now).await?;
    }
    let detail = format!(
        "Agreed: \"{CONSENT}\" Document SHA-256 {}. Signed by {} <{}>.",
        link.request.original_sha256, link.signer.name, link.signer.email
    );
    let event = NewEvent {
        signer_id: Some(link.signer.id),
        ip: client.ip.as_deref(),
        user_agent: client.user_agent.as_deref(),
        detail: Some(&detail),
        ..NewEvent::of(&link.request, EventKind::Signed, now)
    };
    store::insert_event(&mut *tx, event).await?;
    let group = store::next_group(&mut *tx, link.request.workspace_id, link.request.id).await?;
    crate::ops::mail_group(platform, &mut tx, &link.request, &group, now).await?;
    let pending = store::pending_signers(&mut *tx, &link.request).await?;
    if pending == 0 {
        let job = Complete { workspace_id: link.request.workspace_id, request_id: link.request.id };
        platform.bus().enqueue(&mut tx, &job, Enqueue::default()).await?;
    }
    tx.commit().await?;
    Ok(true)
}

fn failed(err: &anyhow::Error) -> Response {
    tracing::error!(error = format!("{err:#}"), "a signer file failed");
    let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
    secure(&mut response, JSON_CSP);
    response
}

pub(crate) fn pdf_response(bytes: Vec<u8>, disposition: &str) -> Response {
    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
    if let Ok(value) = HeaderValue::from_str(disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    // Only pages on this origin may frame the PDF.
    secure(&mut response, "default-src 'none'; frame-ancestors 'self'");
    response
}

/// An ASCII file name for `Content-Disposition`.
pub(crate) fn disposition_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    if clean.is_empty() { "document.pdf".to_owned() } else { clean }
}

async fn document(
    Extension(platform): Extension<Platform>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let bytes = async {
        let Some(link) = resolve(&platform, &query.t).await? else { return Ok(None) };
        if link.request.status == RequestStatus::Voided.as_str() {
            return Ok(None);
        }
        let file =
            store::get_file(platform.db().read(), &link.request, DocumentVersion::Original).await?;
        anyhow::Ok(file.map(|bytes| (bytes, link.request.file_name)))
    }
    .await;
    match bytes {
        Ok(Some((bytes, name))) => {
            pdf_response(bytes, &format!("inline; filename=\"{}\"", disposition_name(&name)))
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => failed(&err),
    }
}

async fn signed_pdf(
    Extension(platform): Extension<Platform>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let bytes = async {
        let Some(link) = resolve(&platform, &query.t).await? else { return Ok(None) };
        if link.request.status != RequestStatus::Completed.as_str() {
            return Ok(None);
        }
        let file =
            store::get_file(platform.db().read(), &link.request, DocumentVersion::Signed).await?;
        anyhow::Ok(file.map(|bytes| (bytes, pdf::signed_file_name(&link.request.file_name))))
    }
    .await;
    match bytes {
        Ok(Some((bytes, name))) => {
            pdf_response(bytes, &format!("attachment; filename=\"{}\"", disposition_name(&name)))
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => failed(&err),
    }
}
