//! The pages signers use, nested at `/m/signing/public`. They sit outside
//! authentication: the link's token is the credential, and the database
//! keeps only its hash. GETs change nothing, so mail scanners that follow a
//! link leave no trace; the page's script reports a view, and the form
//! posts the signature. Everything echoed is escaped.

use anyhow::Context as _;
use axum::extract::{DefaultBodyLimit, Form, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use bizkit_core::Platform;
use bizkit_core::auth::{hash_token, random_token};
use bizkit_core::jobs::Enqueue;
use bizkit_core::net::ClientIp;
use serde::Deserialize;
use utoipa::OpenApi;

use crate::api::{DocumentVersion, EventKind, RequestStatus};
use crate::complete::Complete;
use crate::pdf;
use crate::signature::Signature;
use crate::store::{self, NewEvent, RequestRow, SignerRow};

/// What a signer agrees to; recorded with each signature.
pub(crate) const CONSENT: &str = "I agree to sign this document electronically, and that my electronic \
                                  signature has the same effect as a handwritten one.";
/// Form posts are small: a typed name, or the strokes of one signature.
const MAX_BODY: usize = 256 * 1024;
const MAX_USER_AGENT: usize = 300;

/// The public router.
pub(crate) fn router() -> Router {
    Router::new()
        .route("/sign", get(sign_page).post(sign_post))
        .route("/viewed", post(viewed))
        .route("/document", get(document))
        .route("/signed.pdf", get(signed_pdf))
        .layer(DefaultBodyLimit::max(MAX_BODY))
}

#[derive(OpenApi)]
#[openapi(paths(sign_page_docs, sign_post_docs, viewed_docs, document_docs, signed_pdf_docs))]
pub(crate) struct PublicDocs;

#[utoipa::path(get, path = "/sign", operation_id = "signing.public.sign_page", tag = "signing",
    summary = "The signing page for a signer's link",
    params(("t" = String, Query, description = "The signer's link token")),
    responses((status = 200, description = "The page", content_type = "text/html"),
              (status = 404, description = "The link no longer works", content_type = "text/html")))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn sign_page_docs() {}

#[utoipa::path(post, path = "/sign", operation_id = "signing.public.sign", tag = "signing",
    summary = "Sign with a drawn or typed signature",
    request_body(content_type = "application/x-www-form-urlencoded", content = String,
        description = "`t`, `consent=yes`, `kind` (`drawn` or `typed`), and `strokes` (JSON) or `typed_name`"),
    responses((status = 200, description = "Signed", content_type = "text/html"),
              (status = 404, description = "The link no longer works", content_type = "text/html"),
              (status = 422, description = "The form needs fixing", content_type = "text/html")))]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn sign_post_docs() {}

#[utoipa::path(post, path = "/viewed", operation_id = "signing.public.viewed", tag = "signing",
    summary = "Record that a signer opened the signing page",
    request_body(content_type = "application/x-www-form-urlencoded", content = String, description = "`t`"),
    responses((status = 204, description = "Recorded, or the link no longer works")))]
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

pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
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

const STYLE: &str = "body{font-family:system-ui,sans-serif;max-width:48rem;margin:2rem auto;padding:0 1rem;line-height:1.5}\
    iframe{width:100%;height:70vh;border:1px solid #ccc;border-radius:.375rem}\
    canvas{width:100%;max-width:40rem;aspect-ratio:10/3;border:1px dashed #888;border-radius:.375rem;touch-action:none;background:#fff}\
    button{font:inherit;padding:.5rem 1rem;border-radius:.375rem;border:1px solid #888;cursor:pointer}\
    fieldset{border:1px solid #ccc;border-radius:.375rem;margin:1rem 0}.error{color:#b91c1c}input[type=text]{font:inherit;padding:.4rem}";

fn page(status: StatusCode, title: &str, body: &str, script: Option<(&str, &str)>) -> Response {
    let (script_tag, script_src) = match script {
        Some((nonce, code)) => {
            (format!("<script nonce=\"{nonce}\">{code}</script>"), format!("'nonce-{nonce}'"))
        }
        None => (String::new(), "'none'".to_owned()),
    };
    let html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{title}</title><style>{STYLE}</style></head><body><h1>{title}</h1>{body}{script_tag}</body></html>",
        title = escape(title),
    );
    let mut response = (status, html).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    let csp = format!(
        "default-src 'none'; style-src 'unsafe-inline'; frame-src 'self'; connect-src 'self'; \
         form-action 'self'; base-uri 'none'; frame-ancestors 'none'; script-src {script_src}",
    );
    secure(&mut response, &csp);
    response
}

fn gone() -> Response {
    page(
        StatusCode::NOT_FOUND,
        "This link no longer works",
        "<p>It may have been mistyped, or the request was cancelled. Ask the sender for a new link.</p>",
        None,
    )
}

fn failed(err: &anyhow::Error) -> Response {
    tracing::error!(error = format!("{err:#}"), "signing page failed");
    page(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Something went wrong",
        "<p>Try again in a minute.</p>",
        None,
    )
}

/// The page a signer who signed already sees.
fn done_page(link: &Link, token: &str) -> Response {
    let signed = format!("<p>You signed <strong>{}</strong>.</p>", escape(&link.request.title));
    let next = if link.request.status == RequestStatus::Completed.as_str() {
        format!(
            "<p>Everyone has signed. <a href=\"signed.pdf?t={}\">Download the signed PDF</a>.</p>",
            escape(token)
        )
    } else {
        "<p>You'll get a mail with the signed PDF once everyone has signed.</p>".to_owned()
    };
    page(StatusCode::OK, "Thank you", &format!("{signed}{next}"), None)
}

const PAD_SCRIPT: &str = include_str!("pad.js");

fn sign_form(link: &Link, token: &str, error: Option<&str>) -> Response {
    let request = &link.request;
    let mut body = vec![format!(
        "<p>Hi {name}. You're asked to sign <strong>{file}</strong>.</p>",
        name = escape(&link.signer.name),
        file = escape(&request.file_name)
    )];
    if !request.message.is_empty() {
        body.push(format!("<blockquote>{}</blockquote>", escape(&request.message)));
    }
    let token = escape(token);
    body.push(format!("<iframe src=\"document?t={token}\" title=\"The document\"></iframe>\
         <p><a href=\"document?t={token}\" target=\"_blank\" rel=\"noopener\">Open the PDF in a new tab</a></p>"
    ));
    if let Some(error) = error {
        body.push(format!("<p class=\"error\" role=\"alert\">{}</p>", escape(error)));
    }
    body.push(format!("<form method=\"post\" action=\"sign\" id=\"sign\">\
         <input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <input type=\"hidden\" name=\"strokes\" id=\"strokes\">\
         <fieldset><legend>Your signature</legend>\
         <p><label><input type=\"radio\" name=\"kind\" value=\"typed\" checked> Type your name</label> \
         <label id=\"draw-choice\" hidden><input type=\"radio\" name=\"kind\" value=\"drawn\"> Draw it</label></p>\
         <p id=\"typed\"><label>Full name <input type=\"text\" name=\"typed_name\" maxlength=\"100\" \
         autocomplete=\"name\" value=\"{name}\"></label></p>\
         <div id=\"drawn\" hidden><canvas id=\"pad\" width=\"1000\" height=\"300\" \
         aria-label=\"Draw your signature\"></canvas><p><button type=\"button\" id=\"clear\">Clear</button></p></div>\
         </fieldset>\
         <p><label><input type=\"checkbox\" name=\"consent\" value=\"yes\" required> {consent}</label></p>\
         <p><button type=\"submit\">Sign</button></p></form>",
        name = escape(&link.signer.name),
        consent = escape(CONSENT),
    ));
    let nonce = random_token();
    let status = if error.is_some() { StatusCode::UNPROCESSABLE_ENTITY } else { StatusCode::OK };
    page(status, &request.title, &body.concat(), Some((&nonce, PAD_SCRIPT)))
}

async fn sign_page(
    Extension(platform): Extension<Platform>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let link = match resolve(&platform, &query.t).await {
        Ok(Some(link)) => link,
        Ok(None) => return gone(),
        Err(err) => return failed(&err),
    };
    if link.signer.status == "signed" {
        return done_page(&link, &query.t);
    }
    if link.request.status != RequestStatus::Sent.as_str() {
        return gone();
    }
    sign_form(&link, &query.t, None)
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|agent| agent.chars().take(MAX_USER_AGENT).collect())
}

async fn viewed(
    Extension(platform): Extension<Platform>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Form(form): Form<TokenQuery>,
) -> StatusCode {
    let recorded = async {
        let Some(link) = resolve(&platform, &form.t).await? else { return Ok(()) };
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
    StatusCode::NO_CONTENT
}

#[derive(Debug, Deserialize)]
struct SignForm {
    #[serde(default)]
    t: String,
    #[serde(default)]
    consent: Option<String>,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    strokes: String,
    #[serde(default)]
    typed_name: String,
}

async fn sign_post(
    Extension(platform): Extension<Platform>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Form(form): Form<SignForm>,
) -> Response {
    let link = match resolve(&platform, &form.t).await {
        Ok(Some(link)) => link,
        Ok(None) => return gone(),
        Err(err) => return failed(&err),
    };
    if link.signer.status == "signed" {
        return done_page(&link, &form.t);
    }
    if link.request.status != RequestStatus::Sent.as_str() {
        return gone();
    }
    if form.consent.as_deref() != Some("yes") {
        return sign_form(&link, &form.t, Some("Tick the box to agree to sign electronically."));
    }
    let signature = match Signature::from_form(&form.kind, &form.strokes, &form.typed_name) {
        Ok(signature) => signature,
        Err(problem) => return sign_form(&link, &form.t, Some(problem)),
    };
    let client = Client { ip: ip.map(|ip| ip.to_string()), user_agent: user_agent(&headers) };
    match record_signature(&platform, &link, &signature, &client).await {
        Ok(true) => {
            let signed = Link {
                signer: SignerRow { status: "signed".to_owned(), ..link.signer },
                request: link.request,
            };
            done_page(&signed, &form.t)
        }
        Ok(false) => gone(),
        Err(err) => failed(&err),
    }
}

struct Client {
    ip: Option<String>,
    user_agent: Option<String>,
}

/// Stores the signature and its audit row, and queues the seal after the
/// last one, in one transaction. False when the request or signer changed
/// since the page loaded (voided, or signed in another tab).
async fn record_signature(
    platform: &Platform,
    link: &Link,
    signature: &Signature,
    client: &Client,
) -> anyhow::Result<bool> {
    let now = platform.now();
    let json = serde_json::to_string(signature).context("encoding a signature")?;
    let mut tx = platform.db().write().await?;
    let current = store::find_request(&mut *tx, link.request.workspace_id, link.request.id).await?;
    if current.is_none_or(|row| row.status != RequestStatus::Sent.as_str()) {
        return Ok(false);
    }
    if !store::mark_signed(&mut *tx, &link.signer, &json, now).await? {
        return Ok(false);
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
    let pending = store::pending_signers(&mut *tx, &link.request).await?;
    if pending == 0 {
        let job = Complete { workspace_id: link.request.workspace_id, request_id: link.request.id };
        platform.bus().enqueue(&mut tx, &job, Enqueue::default()).await?;
    }
    tx.commit().await?;
    Ok(true)
}

fn pdf_response(bytes: Vec<u8>, disposition: &str) -> Response {
    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
    if let Ok(value) = HeaderValue::from_str(disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    // Only the signing page, on this origin, may frame the PDF.
    secure(&mut response, "default-src 'none'; frame-ancestors 'self'");
    response
}

/// An ASCII file name for `Content-Disposition`.
fn disposition_name(name: &str) -> String {
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
