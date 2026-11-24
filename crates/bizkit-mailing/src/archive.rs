//! The public archive of a list, nested at `/m/mailing/public/archive`:
//! a page of its archived broadcasts, a page per broadcast and an RSS 2.0
//! feed. They sit outside authentication and only read. A list shows here
//! only while its archive is on, and a broadcast only once it is sent and
//! marked `archived`. Everything echoed is escaped.

use std::fmt::Write as _;

use anyhow::Context as _;
use axum::extract::Path;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::get;
use axum::{Extension, Router};
use bizkit_core::Platform;
use bizkit_core::time::UnixMillis;
use time::format_description::well_known::Rfc2822;
use url::Url;

use crate::api::BroadcastId;
use crate::render::{self, escape};
use crate::store::{self, ArchiveIssue, ListRow};

/// Mail bodies may show web images; nothing else loads.
const CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src http: https:";
const PAGE_ISSUES: i64 = 100;
const FEED_ISSUES: i64 = 20;

/// The archive page of the list whose archive key is `key`.
pub(crate) fn archive_url(platform: &Platform, key: &str) -> anyhow::Result<Url> {
    link(platform, &format!("/m/mailing/public/archive/{key}"))
}

fn feed_url(platform: &Platform, key: &str) -> anyhow::Result<Url> {
    link(platform, &format!("/m/mailing/public/archive/{key}/feed.xml"))
}

fn issue_url(platform: &Platform, key: &str, id: BroadcastId) -> anyhow::Result<Url> {
    link(platform, &format!("/m/mailing/public/archive/{key}/{id}"))
}

fn link(platform: &Platform, path: &str) -> anyhow::Result<Url> {
    platform.base_url().join(path).context("building an archive link")
}

pub(crate) fn router() -> Router {
    Router::new()
        .route("/archive/{key}", get(index))
        .route("/archive/{key}/feed.xml", get(feed))
        .route("/archive/{key}/{broadcast_id}", get(issue))
}

#[utoipa::path(
    get,
    path = "/archive/{key}",
    operation_id = "mailing.public.archive",
    tag = "mailing",
    summary = "Show a list's public archive",
    description = "The list's sent broadcasts marked `archived`, newest first, at most 100.",
    params(("key" = String, Path, description = "The list's archive key, from `archive_url`")),
    responses(
        (status = 200, description = "The page", content_type = "text/html", body = String),
        (status = 404, description = "No list with this key has its archive on", content_type = "text/html", body = String),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn index_docs() {}

#[utoipa::path(
    get,
    path = "/archive/{key}/feed.xml",
    operation_id = "mailing.public.archive_feed",
    tag = "mailing",
    summary = "Get a list's public archive as RSS",
    description = "RSS 2.0 with the latest 20 archived broadcasts; each item's description is its HTML.",
    params(("key" = String, Path, description = "The list's archive key, from `archive_url`")),
    responses(
        (status = 200, description = "The feed", content_type = "application/rss+xml", body = String),
        (status = 404, description = "No list with this key has its archive on", content_type = "text/html", body = String),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn feed_docs() {}

#[utoipa::path(
    get,
    path = "/archive/{key}/{broadcast_id}",
    operation_id = "mailing.public.archive_issue",
    tag = "mailing",
    summary = "Show one broadcast of a list's public archive",
    params(
        ("key" = String, Path, description = "The list's archive key, from `archive_url`"),
        ("broadcast_id" = String, Path, description = "The broadcast"),
    ),
    responses(
        (status = 200, description = "The page", content_type = "text/html", body = String),
        (status = 404, description = "The archive is off, or the broadcast isn't in it", content_type = "text/html", body = String),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn issue_docs() {}

/// The docs for the archive routes, relative to the public nest.
#[derive(utoipa::OpenApi)]
#[openapi(paths(index_docs, feed_docs, issue_docs))]
pub(crate) struct ArchiveDocs;

async fn index(Extension(platform): Extension<Platform>, Path(key): Path<String>) -> Response {
    answer(index_page(&platform, &key).await)
}

async fn feed(Extension(platform): Extension<Platform>, Path(key): Path<String>) -> Response {
    answer(feed_xml(&platform, &key).await)
}

async fn issue(
    Extension(platform): Extension<Platform>,
    Path((key, broadcast_id)): Path<(String, String)>,
) -> Response {
    let Ok(id) = broadcast_id.parse::<BroadcastId>() else { return not_found() };
    answer(issue_page(&platform, &key, id).await)
}

fn answer(result: anyhow::Result<Option<Response>>) -> Response {
    match result {
        Ok(Some(response)) => response,
        Ok(None) => not_found(),
        Err(err) => {
            tracing::error!(error = %format!("{err:#}"), "a mailing archive page failed");
            html(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong",
                None,
                "<p>Please try again later.</p>",
            )
        }
    }
}

async fn index_page(platform: &Platform, key: &str) -> anyhow::Result<Option<Response>> {
    let db = platform.db().read();
    let Some(list) = store::find_archive_list(db, key).await? else { return Ok(None) };
    let issues = store::archive_issues(db, list.workspace_id, list.id, None, PAGE_ISSUES).await?;
    let feed = feed_url(platform, key)?;
    let mut body = format!("<h1>{}</h1>", escape(&list.name));
    if let Some(description) = description(&list) {
        let _ignored = write!(body, "<p>{}</p>", escape(description));
    }
    let _ignored = write!(body, "<p><a href=\"{}\">RSS feed</a></p>", escape(feed.as_str()));
    if issues.is_empty() {
        body.push_str("<p>Nothing here yet.</p>");
    } else {
        body.push_str("<ul>");
        for issue in &issues {
            let _item = write!(
                body,
                "<li><a href=\"{}\">{}</a> <time>{}</time></li>",
                escape(issue_url(platform, key, issue.id)?.as_str()),
                escape(&subject(issue)),
                escape(&day(issue.sent_at)),
            );
        }
        body.push_str("</ul>");
    }
    Ok(Some(html(StatusCode::OK, &list.name, Some(&feed), &body)))
}

async fn issue_page(
    platform: &Platform,
    key: &str,
    id: BroadcastId,
) -> anyhow::Result<Option<Response>> {
    let db = platform.db().read();
    let Some(list) = store::find_archive_list(db, key).await? else { return Ok(None) };
    let Some(issue) =
        store::archive_issues(db, list.workspace_id, list.id, Some(id), 1).await?.pop()
    else {
        return Ok(None);
    };
    let subject = subject(&issue);
    let body = format!(
        "<p><a href=\"{archive}\">{list}</a></p><h1>{title}</h1><p><time>{day}</time></p>\
         <article>{content}</article>",
        archive = escape(archive_url(platform, key)?.as_str()),
        list = escape(&list.name),
        title = escape(&subject),
        day = escape(&day(issue.sent_at)),
        content = render::archive_body(&issue.body_md),
    );
    Ok(Some(html(StatusCode::OK, &subject, Some(&feed_url(platform, key)?), &body)))
}

async fn feed_xml(platform: &Platform, key: &str) -> anyhow::Result<Option<Response>> {
    let db = platform.db().read();
    let Some(list) = store::find_archive_list(db, key).await? else { return Ok(None) };
    let issues = store::archive_issues(db, list.workspace_id, list.id, None, FEED_ISSUES).await?;
    let mut xml =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\"><channel>");
    let _ignored = write!(
        xml,
        "<title>{}</title><link>{}</link><description>{}</description>",
        escape(&list.name),
        escape(archive_url(platform, key)?.as_str()),
        escape(description(&list).unwrap_or(&list.name)),
    );
    for issue in &issues {
        let url = issue_url(platform, key, issue.id)?;
        let _item = write!(
            xml,
            "<item><title>{title}</title><link>{url}</link><guid isPermaLink=\"true\">{url}</guid>\
             <pubDate>{date}</pubDate><description>{content}</description></item>",
            title = escape(&subject(issue)),
            url = escape(url.as_str()),
            date = escape(&rfc2822(issue.sent_at)?),
            content = escape(&render::archive_body(&issue.body_md)),
        );
    }
    xml.push_str("</channel></rss>\n");
    let mut response = (StatusCode::OK, xml).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/rss+xml; charset=utf-8"),
    );
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(Some(response))
}

fn description(list: &ListRow) -> Option<&str> {
    list.description.as_deref().map(str::trim).filter(|text| !text.is_empty())
}

fn subject(issue: &ArchiveIssue) -> String {
    render::archive_subject(&issue.subject)
}

/// `2026-11-24`, in UTC.
fn day(at: UnixMillis) -> String {
    at.to_datetime().map_or_else(String::new, |at| at.date().to_string())
}

fn rfc2822(at: UnixMillis) -> anyhow::Result<String> {
    at.to_datetime()
        .context("a send time out of range")?
        .format(&Rfc2822)
        .context("formatting a send time")
}

fn not_found() -> Response {
    html(
        StatusCode::NOT_FOUND,
        "This archive is not available",
        None,
        "<h1>This archive is not available</h1><p>The link may be mistyped, or the archive was turned off.</p>",
    )
}

fn html(status: StatusCode, title: &str, feed: Option<&Url>, body: &str) -> Response {
    let alternate = feed.map_or_else(String::new, |url| {
        format!(
            "<link rel=\"alternate\" type=\"application/rss+xml\" href=\"{}\">",
            escape(url.as_str())
        )
    });
    let page = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"referrer\" content=\"no-referrer\"><title>{title}</title>{alternate}\
         <style>body{{font-family:system-ui,sans-serif;max-width:42rem;margin:3rem auto;padding:0 1rem;\
         line-height:1.6;color:#111827}}a{{color:#1d4ed8}}time{{color:#6b7280;font-size:.875rem}}\
         ul{{list-style:none;padding:0}}li{{padding:.5rem 0;border-bottom:1px solid #e5e7eb}}\
         img{{max-width:100%}}</style></head><body>{body}</body></html>",
        title = escape(title),
    );
    let mut response = (status, page).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}
