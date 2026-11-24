//! The confirm and unsubscribe pages, nested at `/m/mailing/public`. They
//! sit outside authentication. GETs only show a button, so mail scanners
//! that follow links don't confirm or unsubscribe anyone; the POSTs change
//! state. Everything echoed is escaped.

use anyhow::Context as _;
use axum::body::Bytes;
use axum::extract::Query;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::get;
use axum::{Extension, Router};
use bizkit_core::Platform;
use serde::Deserialize;
use url::Url;
use utoipa::OpenApi;

use crate::contacts::{self, LinkOutcome};
use crate::render::escape;

/// Pages post only to themselves and load nothing.
const CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'";
/// The body mail clients send for RFC 8058 one-click unsubscribe.
const ONE_CLICK: &str = "List-Unsubscribe=One-Click";

/// Where a confirm link points.
pub(crate) fn confirm_url(platform: &Platform, token: &str) -> anyhow::Result<Url> {
    link(platform, "confirm", token)
}

/// Where an unsubscribe link points; also the `List-Unsubscribe` URL.
pub(crate) fn unsubscribe_url(platform: &Platform, token: &str) -> anyhow::Result<Url> {
    link(platform, "unsubscribe", token)
}

fn link(platform: &Platform, page: &str, token: &str) -> anyhow::Result<Url> {
    let mut url = platform
        .base_url()
        .join(&format!("/m/mailing/public/{page}"))
        .context("building a mailing link")?;
    url.query_pairs_mut().append_pair("t", token);
    Ok(url)
}

/// The public router.
pub(crate) fn router() -> Router {
    Router::new()
        .route("/confirm", get(confirm_page).post(confirm_post))
        .route("/unsubscribe", get(unsubscribe_page).post(unsubscribe_post))
}

/// The token a link carries.
#[derive(Debug, Deserialize)]
struct Token {
    #[serde(default)]
    t: String,
}

#[utoipa::path(
    get,
    path = "/confirm",
    operation_id = "mailing.public.confirm_page",
    tag = "mailing",
    summary = "Show the confirm-subscription page",
    description = "A page with a button that confirms. Changes nothing.",
    params(("t" = String, Query, description = "The token from the confirm mail")),
    responses(
        (status = 200, description = "The page", content_type = "text/html", body = String),
        (status = 404, description = "The link is unknown or expired", content_type = "text/html", body = String),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn confirm_page_docs() {}

#[utoipa::path(
    post,
    path = "/confirm",
    operation_id = "mailing.public.confirm",
    tag = "mailing",
    summary = "Confirm a subscription",
    params(("t" = String, Query, description = "The token from the confirm mail")),
    responses(
        (status = 200, description = "Subscribed", content_type = "text/html", body = String),
        (status = 404, description = "The link is unknown or expired", content_type = "text/html", body = String),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn confirm_docs() {}

#[utoipa::path(
    get,
    path = "/unsubscribe",
    operation_id = "mailing.public.unsubscribe_page",
    tag = "mailing",
    summary = "Show the unsubscribe page",
    description = "A page with a button that unsubscribes. Changes nothing.",
    params(("t" = String, Query, description = "The token from a list mail")),
    responses(
        (status = 200, description = "The page", content_type = "text/html", body = String),
        (status = 404, description = "The link is unknown", content_type = "text/html", body = String),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn unsubscribe_page_docs() {}

#[utoipa::path(
    post,
    path = "/unsubscribe",
    operation_id = "mailing.public.unsubscribe",
    tag = "mailing",
    summary = "Unsubscribe from a list",
    description = "The `List-Unsubscribe` URL. A body of `List-Unsubscribe=One-Click` (RFC 8058) gets a plain-text answer; the page's button gets a page.",
    params(("t" = String, Query, description = "The token from a list mail")),
    request_body(content((String = "application/x-www-form-urlencoded"), (String = "multipart/form-data")), description = "Optional; `List-Unsubscribe=One-Click` asks for a plain-text answer"),
    responses(
        (status = 200, description = "Unsubscribed", content(
            (String = "text/html"),
            (String = "text/plain"),
        )),
        (status = 404, description = "The link is unknown", content(
            (String = "text/html"),
            (String = "text/plain"),
        )),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn unsubscribe_docs() {}

/// The docs for the public routes, relative to the nest.
#[derive(OpenApi)]
#[openapi(paths(confirm_page_docs, confirm_docs, unsubscribe_page_docs, unsubscribe_docs))]
pub(crate) struct PublicDocs;

fn page(status: StatusCode, title: &str, body: &str) -> Response {
    let page = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"referrer\" content=\"no-referrer\"><title>{title}</title>\
         <style>body{{font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;line-height:1.5}}\
         button{{font:inherit;padding:.5rem 1rem;border-radius:.375rem;border:1px solid #888;cursor:pointer}}</style>\
         </head><body><h1>{title}</h1>{body}</body></html>",
        title = escape(title),
    );
    let mut response = (status, page).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn invalid() -> Response {
    page(
        StatusCode::NOT_FOUND,
        "This link is not valid",
        "<p>It may have been mistyped or replaced by a newer one.</p>",
    )
}

fn failed(err: &anyhow::Error) -> Response {
    tracing::error!(error = %format!("{err:#}"), "a mailing link failed");
    page(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Something went wrong",
        "<p>Please try again later.</p>",
    )
}

fn button(action: &str, token: &str, label: &str) -> String {
    let mut target = format!("{action}?");
    target.push_str(
        &url::form_urlencoded::Serializer::new(String::new()).append_pair("t", token).finish(),
    );
    format!(
        "<form method=\"post\" action=\"{}\"><button type=\"submit\">{}</button></form>",
        escape(&target),
        escape(label)
    )
}

async fn confirm_page(
    Extension(platform): Extension<Platform>,
    Query(token): Query<Token>,
) -> Response {
    let db = platform.db().read();
    let contact = match crate::store::find_by_confirm_token(db, &token.t).await {
        Ok(Some(contact)) => contact,
        Ok(None) => return invalid(),
        Err(err) => return failed(&err.into()),
    };
    let list = match crate::store::find_list(db, contact.workspace_id, contact.list_id).await {
        Ok(Some(list)) => list,
        Ok(None) => return invalid(),
        Err(err) => return failed(&err.into()),
    };
    match contact.status.as_str() {
        "subscribed" => subscribed(&list.name),
        "pending" => {
            let expired = contact
                .confirm_sent_at
                .is_none_or(|at| at <= platform.now().minus(contacts::CONFIRM_TTL));
            if expired {
                return expired_page();
            }
            let body = format!(
                "<p>Confirm that you want mail from <strong>{}</strong>.</p>{}",
                escape(&list.name),
                button("confirm", &token.t, "Confirm subscription")
            );
            page(StatusCode::OK, "Confirm your subscription", &body)
        }
        _ => invalid(),
    }
}

fn subscribed(list: &str) -> Response {
    let body = format!("<p>You'll get mail from <strong>{}</strong>.</p>", escape(list));
    page(StatusCode::OK, "You're subscribed", &body)
}

fn expired_page() -> Response {
    page(
        StatusCode::NOT_FOUND,
        "This link has expired",
        "<p>Confirm links work for 7 days. Sign up again to get a new one.</p>",
    )
}

async fn confirm_post(
    Extension(platform): Extension<Platform>,
    Query(token): Query<Token>,
) -> Response {
    let result = async {
        let mut tx = platform.db().write().await?;
        let outcome = contacts::confirm(&mut tx, &token.t, platform.now()).await?;
        tx.commit().await?;
        anyhow::Ok(outcome)
    }
    .await;
    match result {
        Ok(LinkOutcome::Done(list) | LinkOutcome::Already(list)) => subscribed(&list),
        Ok(LinkOutcome::Expired) => expired_page(),
        Ok(LinkOutcome::Invalid) => invalid(),
        Err(err) => failed(&err),
    }
}

async fn unsubscribe_page(
    Extension(platform): Extension<Platform>,
    Query(token): Query<Token>,
) -> Response {
    match contacts::unsubscribe_target(platform.db().read(), &token.t).await {
        Ok(LinkOutcome::Done(list)) => {
            let body = format!(
                "<p>Stop getting mail from <strong>{}</strong>?</p>{}",
                escape(&list),
                button("unsubscribe", &token.t, "Unsubscribe")
            );
            page(StatusCode::OK, "Unsubscribe", &body)
        }
        Ok(LinkOutcome::Already(list)) => unsubscribed(&list),
        Ok(LinkOutcome::Expired | LinkOutcome::Invalid) => invalid(),
        Err(err) => failed(&err),
    }
}

fn unsubscribed(list: &str) -> Response {
    let body = format!("<p>You won't get more mail from <strong>{}</strong>.</p>", escape(list));
    page(StatusCode::OK, "You're unsubscribed", &body)
}

fn plain(status: StatusCode, text: &'static str) -> Response {
    let mut response = (status, text).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    response
}

/// Whether a POST body asks for RFC 8058 one-click unsubscribe. Mail
/// providers send it URL-encoded or as `multipart/form-data` (RFC 8058's own
/// example), so the body is read whatever its content type: any POST with a
/// valid token unsubscribes, and this only picks the plain-text answer.
fn is_one_click(body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(body);
    text.trim() == ONE_CLICK
        || (text.contains("name=\"List-Unsubscribe\"") && text.contains("One-Click"))
}

async fn unsubscribe_post(
    Extension(platform): Extension<Platform>,
    Query(token): Query<Token>,
    body: Bytes,
) -> Response {
    let one_click = is_one_click(&body);
    let result = async {
        let mut tx = platform.db().write().await?;
        let outcome = contacts::unsubscribe_by_token(&mut tx, &token.t, platform.now()).await?;
        tx.commit().await?;
        anyhow::Ok(outcome)
    }
    .await;
    match (result, one_click) {
        (Ok(LinkOutcome::Done(_) | LinkOutcome::Already(_)), true) => {
            plain(StatusCode::OK, "Unsubscribed.\n")
        }
        (Ok(LinkOutcome::Done(list) | LinkOutcome::Already(list)), false) => unsubscribed(&list),
        (Ok(LinkOutcome::Expired | LinkOutcome::Invalid), true) => {
            plain(StatusCode::NOT_FOUND, "This link is not valid.\n")
        }
        (Ok(LinkOutcome::Expired | LinkOutcome::Invalid), false) => invalid(),
        (Err(err), _) => failed(&err),
    }
}
