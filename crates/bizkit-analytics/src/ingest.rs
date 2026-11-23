//! The public routes, nested at `/m/analytics/public`: the snippet each site
//! loads, and the beacon endpoint it sends to. The beacon always answers
//! 202 with no body, so a site can't tell stored events from dropped ones.

use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Path;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use bizkit_core::Platform;
use bizkit_core::net::ClientIp;
use utoipa::OpenApi;

use crate::api::{Day, IdentityMode};
use crate::beacon::{self, Beacon, Name};
use crate::cache::{CachedSite, Salts, Sites};
use crate::enrich::{self, Who};
use crate::geo::Geo;
use crate::limit::Limiter;
use crate::names;
use crate::write::{self, Incoming, Origin};

const SNIPPET: &str = include_str!("snippet.js");
/// How long browsers keep a site's snippet. Identity mode changes reach
/// visitors within this time.
const SNIPPET_MAX_AGE: &str = "public, max-age=300";

/// The module's shared ingest state: caches, the rate limiter and the country lookup.
#[derive(Debug, Default)]
pub(crate) struct Ingest {
    pub(crate) sites: Sites,
    pub(crate) salts: Salts,
    pub(crate) limiter: Limiter,
    pub(crate) geo: Geo,
}

impl Ingest {
    pub(crate) fn new(geo: Geo) -> Self {
        Self { geo, ..Self::default() }
    }
}

/// The public router.
pub(crate) fn router(ingest: Arc<Ingest>) -> Router {
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE])
        .max_age(Duration::from_hours(1));
    Router::new()
        .route("/e", post(collect))
        .route("/s/{file}", get(script))
        .layer(Extension(ingest))
        .layer(cors)
}

/// The docs for the public routes, relative to the nest.
#[derive(OpenApi)]
#[openapi(paths(collect_docs, script_docs))]
pub(crate) struct IngestDocs;

#[utoipa::path(
    post,
    path = "/e",
    operation_id = "analytics.public.collect",
    tag = "analytics",
    summary = "Record an analytics event from the snippet",
    description = "Takes one event as a JSON text body of at most 8 KiB, sent as `text/plain` so browsers skip the CORS preflight. Always answers 202, whether the event was stored or dropped (bots, unknown sites, other hosts, rate limits, malformed bodies).",
    request_body(content((String = "text/plain"))),
    responses((status = 202, description = "Received")),
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn collect_docs() {}

#[utoipa::path(
    get,
    path = "/s/{file}",
    operation_id = "analytics.public.script",
    tag = "analytics",
    summary = "Get a site's tracking snippet",
    params(("file" = String, Path, description = "The site key and `.js`")),
    responses(
        (status = 200, description = "The snippet", content_type = "text/javascript", body = String),
        (status = 404, description = "No site has this key"),
    ),
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
const fn script_docs() {}

async fn script(
    Extension(platform): Extension<Platform>,
    Extension(ingest): Extension<Arc<Ingest>>,
    Path(file): Path<String>,
) -> Response {
    let Some(key) = file.strip_suffix(".js") else { return StatusCode::NOT_FOUND.into_response() };
    let site = match ingest.sites.get(platform.db(), key, platform.now()).await {
        Ok(Some(site)) => site,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(error = format!("{err:#}"), "loading a site for its snippet");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let body = match render(&platform, &site) {
        Ok(body) => body,
        Err(err) => {
            tracing::error!(error = format!("{err:#}"), "rendering a snippet");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/javascript; charset=utf-8"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(SNIPPET_MAX_AGE));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

fn render(platform: &Platform, site: &CachedSite) -> anyhow::Result<String> {
    let endpoint = platform.base_url().join("/m/analytics/public/e")?;
    let json = |text: &str| serde_json::to_string(text);
    Ok(SNIPPET
        .replace("__SITE__", &json(&site.row.site_key)?)
        .replace("__MODE__", &json(site.identity_mode.as_str())?)
        .replace("__ENDPOINT__", &json(endpoint.as_str())?))
}

/// Why a beacon wasn't stored. Only logged.
#[derive(Debug)]
enum Dropped {
    Parse(beacon::Reject),
    Bot,
    UnknownSite,
    Host,
    Limited,
    Cookieless,
}

impl fmt::Display for Dropped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(reason) => write!(f, "{reason}"),
            Self::Bot => f.write_str("bot user agent"),
            Self::UnknownSite => f.write_str("unknown site key"),
            Self::Host => f.write_str("host not allowed"),
            Self::Limited => f.write_str("rate limited"),
            Self::Cookieless => f.write_str("identify on a cookieless site"),
        }
    }
}

async fn collect(
    Extension(platform): Extension<Platform>,
    Extension(ingest): Extension<Arc<Ingest>>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    body: Body,
) -> StatusCode {
    match accept(&platform, &ingest, ip, &headers, body).await {
        Ok(Ok(())) => {}
        Ok(Err(reason)) => tracing::debug!(%reason, "dropped an analytics beacon"),
        Err(err) => {
            tracing::warn!(error = format!("{err:#}"), "storing an analytics beacon failed");
        }
    }
    StatusCode::ACCEPTED
}

/// Checks a beacon and stores it. The outer error is a failure on our side;
/// the inner one a beacon we chose not to store.
async fn accept(
    platform: &Platform,
    ingest: &Ingest,
    ip: Option<IpAddr>,
    headers: &HeaderMap,
    body: Body,
) -> anyhow::Result<Result<(), Dropped>> {
    let Ok(bytes) = axum::body::to_bytes(body, beacon::MAX_BODY).await else {
        return Ok(Err(Dropped::Parse(beacon::Reject::TooLarge)));
    };
    let mut beacon = match beacon::parse(&bytes) {
        Ok(beacon) => beacon,
        Err(reason) => return Ok(Err(Dropped::Parse(reason))),
    };
    let ua = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
    if crate::ua::is_bot(ua) {
        return Ok(Err(Dropped::Bot));
    }
    let now = platform.now();
    let Some(site) = ingest.sites.get(platform.db(), &beacon.site, now).await? else {
        return Ok(Err(Dropped::UnknownSite));
    };
    let host = beacon.url.host_str().unwrap_or("").to_ascii_lowercase();
    if !names::host_allowed(&site.hostnames, &host) {
        return Ok(Err(Dropped::Host));
    }
    if site.identity_mode == IdentityMode::Cookieless {
        if beacon.name == Name::Identify {
            return Ok(Err(Dropped::Cookieless));
        }
        beacon.anon_id = None;
        beacon.user_id = None;
        beacon.traits = None;
    }
    if !ingest.limiter.allow(site.row.project_id, ip, now) {
        return Ok(Err(Dropped::Limited));
    }
    let event = enrich_beacon(ingest, platform, &site, beacon, Who::Web { ip, ua }).await?;
    write::record(platform, &ingest.sites, &site, &[event]).await?;
    Ok(Ok(()))
}

async fn enrich_beacon(
    ingest: &Ingest,
    platform: &Platform,
    site: &CachedSite,
    beacon: Beacon,
    who: Who<'_>,
) -> anyhow::Result<Incoming> {
    let Who::Web { ip, ua } = who else { anyhow::bail!("beacons come from browsers") };
    let salt = ingest.salts.get(platform.db(), Day::of(platform.now())).await?;
    let country = match ip {
        Some(ip) => ingest.geo.country(ip).await,
        None => None,
    };
    Ok(Incoming {
        attribution: enrich::source(&beacon.url, beacon.referrer.as_ref()),
        path: Some(enrich::path(&beacon.url)),
        host: beacon.url.host_str().map(str::to_ascii_lowercase),
        country,
        agent: Some(crate::ua::parse(ua, beacon.width)),
        visitor: enrich::visitor(&salt, site.row.project_id, who),
        name: beacon.name,
        anon_id: beacon.anon_id,
        user_id: beacon.user_id,
        props: beacon.props,
        traits: beacon.traits,
        origin: Origin::Web,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_snippet_stays_small() {
        assert!(super::SNIPPET.len() < 4096, "{} bytes", super::SNIPPET.len());
        for placeholder in ["__SITE__", "__MODE__", "__ENDPOINT__"] {
            assert!(super::SNIPPET.contains(placeholder), "{placeholder}");
        }
    }
}
