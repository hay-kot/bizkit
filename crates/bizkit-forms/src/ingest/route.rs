//! The public ingest routes, nested at `/m/forms/public`: the submit
//! endpoint static sites post to, and the challenge the ALTCHA widget loads.
//!
//! Answers depend on the caller. JSON clients (a JSON body, or `Accept`
//! naming JSON) get JSON. Browser form posts get a redirect or a small HTML
//! page; those pages share the app's origin, so every echoed value is
//! escaped and the responses forbid scripts.

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Path;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use bizkit_core::error::ErrorBody;
use bizkit_core::jobs::{Enqueue, SendMail};
use bizkit_core::{ApiError, Platform};
use rand::RngExt as _;
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};
use url::Url;
use utoipa::{OpenApi, ToSchema};

use super::bot::{self, ALTCHA_FIELD, Challenge, ChallengeSettings, SpamReason, VerifiedProof};
use super::parse::{self, LIMITS, RawFields};
use crate::api::{BotCheck, FieldValue, FormId, SubmissionId, SubmissionStatus};
use crate::hooks::{HookInput, HookVerdict, SubmissionCreated, SubmitHook};
use crate::store::{self, FormRow, SubmissionRow};
use crate::{fields, notify, ops};

/// The proof-of-work difficulty: the largest number a solver tries.
pub const POW_MAX_NUMBER: u32 = 50_000;
/// Inbox submissions per form per hour that get notification mail.
pub const NOTIFY_PER_HOUR: i64 = 30;
const HEADER_CHARS: usize = 512;
const CSP: &str = "default-src 'none'; style-src 'unsafe-inline'";

/// The public router.
pub(crate) fn router(hook: Arc<dyn SubmitHook>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT])
        .max_age(Duration::from_hours(1));
    Router::new()
        .route("/{form_id}", post(submit))
        .route("/{form_id}/challenge", get(challenge))
        .layer(Extension(hook))
        .layer(cors)
}

/// The docs for the public routes, relative to the nest.
#[derive(OpenApi)]
#[openapi(paths(submit_docs, challenge_docs), components(schemas(Accepted, Challenge, ErrorBody)))]
pub(crate) struct IngestDocs;

/// A submission was taken.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Accepted {
    /// Always true.
    pub ok: bool,
}

#[utoipa::path(
    post,
    path = "/{form_id}",
    operation_id = "forms.public.submit",
    tag = "forms",
    summary = "Submit a form",
    description = "Takes an `application/x-www-form-urlencoded` or `application/json` body of at most 64 KiB. JSON clients (a JSON body or an `Accept` naming JSON) get JSON; browser form posts get a redirect or an HTML page. Submissions that fail the bot check answer like accepted ones.",
    params(("form_id" = String, Path, description = "The form ID")),
    request_body(content(
        (String = "application/x-www-form-urlencoded"),
        (Object = "application/json"),
    )),
    responses(
        (status = 201, description = "Taken (JSON clients)", body = Accepted),
        (status = 200, description = "Taken: a thank-you page", content_type = "text/html", body = String),
        (status = 303, description = "Taken: go to the form's redirect URL"),
        (status = 400, description = "The body is malformed or too large", content(
            (ErrorBody = "application/json"),
            (String = "text/html"),
        )),
        (status = 404, description = "No enabled form has this ID", content(
            (ErrorBody = "application/json"),
            (String = "text/html"),
        )),
        (status = 422, description = "Fields failed validation", content(
            (ErrorBody = "application/json"),
            (String = "text/html"),
        )),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn submit_docs() {}

#[utoipa::path(
    get,
    path = "/{form_id}/challenge",
    operation_id = "forms.public.challenge",
    tag = "forms",
    summary = "Get an ALTCHA challenge for a form",
    params(("form_id" = String, Path, description = "The form ID")),
    responses(
        (status = 200, description = "A challenge valid for one hour", body = Challenge),
        (status = 404, description = "No enabled challenge form has this ID", body = ErrorBody),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn challenge_docs() {}

/// How the caller wants its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reply {
    Json,
    Html,
}

impl Reply {
    fn of(headers: &HeaderMap) -> Self {
        let text =
            |name| headers.get(name).and_then(|v: &HeaderValue| v.to_str().ok()).unwrap_or("");
        let json = |value: &str| value.to_ascii_lowercase().contains("application/json");
        if json(text(header::CONTENT_TYPE)) || json(text(header::ACCEPT)) {
            Self::Json
        } else {
            Self::Html
        }
    }

    fn error(self, err: ApiError, headers: &HeaderMap) -> Response {
        match self {
            Self::Json => err.into_response(),
            Self::Html => {
                let status = err.code().status();
                let mut body = String::new();
                if let ApiError::Validation(errors) = &err {
                    body.push_str("<ul>");
                    for (field, messages) in errors.as_map() {
                        for message in messages {
                            let _ignored = write!(
                                body,
                                "<li><b>{}</b> {}</li>",
                                escape(field),
                                escape(message)
                            );
                        }
                    }
                    body.push_str("</ul>");
                } else {
                    let _ignored = write!(body, "<p>{}</p>", escape(&err.to_string()));
                }
                body.push_str(&back_link(headers));
                html(status, "Your submission didn't go through", &body)
            }
        }
    }
}

/// A link back to the sending page, when it is an http(s) URL.
fn back_link(headers: &HeaderMap) -> String {
    headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Url::parse(v).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map_or_else(
            || "<p>Go back and try again.</p>".to_owned(),
            |url| format!("<p><a href=\"{}\">Go back and try again</a></p>", escape(url.as_str())),
        )
}

fn html(status: StatusCode, title: &str, body: &str) -> Response {
    let page = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{title}</title>\
         <style>body{{font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;line-height:1.5}}</style>\
         </head><body><h1>{title}</h1>{body}</body></html>",
        title = escape(title),
    );
    let mut response = (status, page).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

/// HTML-escapes text for element content and quoted attributes.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn header_text(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    let value = headers.get(name)?.to_str().ok()?;
    Some(value.chars().take(HEADER_CHARS).collect())
}

fn request_id(headers: &HeaderMap) -> String {
    headers.get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or("-").to_owned()
}

async fn find_form(platform: &Platform, form_id: &str) -> Result<FormRow, ApiError> {
    let Ok(id) = form_id.parse::<FormId>() else { return Err(ApiError::NotFound) };
    store::find_public(platform.db().read(), id).await?.ok_or(ApiError::NotFound)
}

fn settings(form: &FormRow) -> ChallengeSettings<'_> {
    ChallengeSettings {
        form_id: form.id,
        key: &form.challenge_key,
        max_number: POW_MAX_NUMBER,
        min_fill_ms: ops::min_fill_ms(form),
    }
}

async fn challenge(
    Extension(platform): Extension<Platform>,
    Path(form_id): Path<String>,
) -> Response {
    let form = match find_form(&platform, &form_id).await {
        Ok(form) if ops::bot_check(&form) == BotCheck::Challenge => form,
        Ok(_) => return ApiError::NotFound.into_response(),
        Err(err) => return err.into_response(),
    };
    let random = (rand::rng().random::<[u8; 16]>(), rand::rng().random::<u32>());
    let challenge = bot::issue(settings(&form), platform.now(), random);
    let mut response = Json(challenge).into_response();
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// What the checks decided before anything is written.
#[derive(Debug)]
struct Decision {
    status: SubmissionStatus,
    spam_reason: Option<String>,
    data: std::collections::BTreeMap<String, FieldValue>,
    proof: Option<VerifiedProof>,
}

async fn submit(
    Extension(platform): Extension<Platform>,
    Extension(hook): Extension<Arc<dyn SubmitHook>>,
    Path(form_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let reply = Reply::of(&headers);
    let request_id = request_id(&headers);
    match take(&platform, hook.as_ref(), &form_id, &headers, body).await {
        Ok(form) => accepted(reply, &form),
        Err(err) => {
            err.log(&request_id);
            reply.error(err, &headers)
        }
    }
}

fn accepted(reply: Reply, form: &FormRow) -> Response {
    match reply {
        Reply::Json => (StatusCode::CREATED, Json(Accepted { ok: true })).into_response(),
        Reply::Html => form
            .redirect_url
            .as_deref()
            .and_then(|url| HeaderValue::from_str(url).ok())
            .map_or_else(
                || html(StatusCode::OK, "Thanks!", "<p>Your submission was received.</p>"),
                |location| {
                    let mut response = StatusCode::SEE_OTHER.into_response();
                    response.headers_mut().insert(header::LOCATION, location);
                    response
                },
            ),
    }
}

async fn take(
    platform: &Platform,
    hook: &dyn SubmitHook,
    form_id: &str,
    headers: &HeaderMap,
    body: Body,
) -> Result<FormRow, ApiError> {
    let form = find_form(platform, form_id).await?;
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok());
    let kind = parse::body_kind(content_type).map_err(|err| bad_request(&err))?;
    let bytes = axum::body::to_bytes(body, LIMITS.body_bytes)
        .await
        .map_err(|_| bad_request(&parse::ParseError::TooLarge))?;
    let raw = parse::parse(kind, &bytes).map_err(|err| bad_request(&err))?;
    let decision = decide(platform, hook, &form, raw)?;
    store_submission(platform, &form, decision, headers).await?;
    Ok(form)
}

fn bad_request(err: &parse::ParseError) -> ApiError {
    ApiError::BadRequest(err.to_string())
}

/// Runs the bot checks, the hook and field validation.
fn decide(
    platform: &Platform,
    hook: &dyn SubmitHook,
    form: &FormRow,
    raw: RawFields,
) -> Result<Decision, ApiError> {
    let mut proof = None;
    let mut spam = bot::honeypot(&raw).map(|reason| reason.as_str().to_owned());
    if spam.is_none() && ops::bot_check(form) == BotCheck::Challenge {
        let altcha = raw.0.get(ALTCHA_FIELD).and_then(|values| values.first()).map(String::as_str);
        match bot::verify(settings(form), altcha, platform.now()) {
            Ok(verified) => proof = Some(verified),
            Err(reason) => spam = Some(reason.as_str().to_owned()),
        }
    }
    if spam.is_none() {
        let input = HookInput {
            workspace_id: form.workspace_id,
            form_id: form.id,
            form_name: &form.name,
            fields: &raw,
        };
        match hook.pre_validate(&input) {
            HookVerdict::Accept => {}
            HookVerdict::Reject(errors) => return Err(errors.into()),
            HookVerdict::Spam(reason) => spam = Some(reason),
        }
    }
    let defs = ops::decode_fields(form)?;
    if let Some(reason) = spam {
        return Ok(Decision {
            status: SubmissionStatus::Spam,
            spam_reason: Some(reason),
            data: fields::keep_all(raw),
            proof,
        });
    }
    let data = fields::validate(&defs, raw).map_err(ApiError::Validation)?;
    Ok(Decision { status: SubmissionStatus::Inbox, spam_reason: None, data, proof })
}

async fn store_submission(
    platform: &Platform,
    form: &FormRow,
    mut decision: Decision,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let now = platform.now();
    let mut tx = platform.db().write().await?.with_request_id(request_id(headers));
    if let Some(proof) = &decision.proof
        && !store::use_challenge(&mut tx, &proof.challenge, proof.expires_at, now).await?
    {
        decision.status = SubmissionStatus::Spam;
        decision.spam_reason = Some(SpamReason::Replayed.as_str().to_owned());
    }
    let recent = store::recent_inbox(
        &mut *tx,
        form.workspace_id,
        form.id,
        now.minus(Duration::from_hours(1)),
    )
    .await?;
    let row = SubmissionRow {
        id: SubmissionId::new(),
        workspace_id: form.workspace_id,
        form_id: form.id,
        status: decision.status.as_str().to_owned(),
        spam_reason: decision.spam_reason,
        data: serde_json::to_string(&decision.data).map_err(anyhow::Error::from)?,
        user_agent: header_text(headers, header::USER_AGENT),
        referer: header_text(headers, header::REFERER),
        created_at: now,
    };
    store::insert_submission(&mut *tx, &row).await?;
    if decision.status == SubmissionStatus::Inbox {
        let event = SubmissionCreated {
            workspace_id: form.workspace_id,
            form_id: form.id,
            submission_id: row.id,
        };
        platform.bus().publish(&mut tx, &event).await?;
        if recent < NOTIFY_PER_HOUR {
            let link = platform
                .base_url()
                .join(&format!("/w/{}/forms/{}", form.workspace_id, form.id))
                .map_err(anyhow::Error::from)?;
            let defs = ops::decode_fields(form)?;
            for to in ops::decode_notify(form)? {
                let mail = notify::mail(&form.name, &defs, &decision.data, &link, &to);
                platform.bus().enqueue(&mut tx, &SendMail(mail), Enqueue::default()).await?;
            }
        } else {
            tracing::warn!(form_id = %form.id, "notification cap reached; storing without mail");
        }
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup() {
        assert_eq!(
            escape("<a href=\"x\">'&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;",
            "escaped"
        );
    }

    #[test]
    fn json_callers() {
        let mut headers = HeaderMap::new();
        assert_eq!(Reply::of(&headers), Reply::Html, "plain form post");
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json, */*"));
        assert_eq!(Reply::of(&headers), Reply::Json, "accept");
        let mut json_body = HeaderMap::new();
        json_body.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        assert_eq!(Reply::of(&json_body), Reply::Json, "json body");
    }
}
