//! The public capture route, nested at `/m/webhooks/public`. It stores
//! every request an enabled endpoint gets and answers `{"ok": true}`. The
//! answer never echoes request data.

use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, RawQuery};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{MethodFilter, on};
use axum::{Extension, Json, Router};
use bizkit_core::error::ErrorBody;
use bizkit_core::jobs::Enqueue;
use bizkit_core::{ApiError, Platform};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::api::{EndpointId, RequestId};
use crate::deliver::Forward;
use crate::hooks::RequestReceived;
use crate::store::{self, EndpointRow, RequestRow};

/// The largest body an endpoint stores.
pub const MAX_BODY_BYTES: usize = 256 * 1024;
/// Requests kept per endpoint; older ones go as new ones arrive.
pub const KEEP_PER_ENDPOINT: i64 = 500;
/// Requests an endpoint stores per hour before it answers 429. Below
/// [`KEEP_PER_ENDPOINT`], since the count comes from the kept rows.
pub const STORED_PER_HOUR: i64 = 300;
const MAX_HEADERS: usize = 100;
/// Longer header values are dropped, not cut, so a forward never sends an
/// altered header.
const HEADER_CHARS: usize = 4096;
/// Standard credential headers, hop-by-hop headers and the request ID this
/// server adds. Never stored.
const DROPPED_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "proxy-authorization",
    "host",
    "connection",
    "content-length",
    "transfer-encoding",
    "keep-alive",
    "upgrade",
    "te",
    "trailer",
    "proxy-connection",
    "x-request-id",
];

/// The public router.
pub(crate) fn router() -> Router {
    let methods = MethodFilter::GET
        .or(MethodFilter::POST)
        .or(MethodFilter::PUT)
        .or(MethodFilter::PATCH)
        .or(MethodFilter::DELETE);
    Router::new().route("/{endpoint_id}", on(methods, capture))
}

/// A request was stored.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Captured {
    /// Always true.
    pub ok: bool,
}

macro_rules! capture_docs {
    ($($method:ident => $name:ident, $id:literal);+ $(;)?) => {$(
        #[utoipa::path(
            $method,
            path = "/{endpoint_id}",
            operation_id = $id,
            tag = "webhooks",
            summary = "Send a request to a capture endpoint",
            description = "Stores the method, query, headers and a body of at most 256 KiB. `Authorization`, `Cookie` and `Proxy-Authorization` are never stored.",
            params(("endpoint_id" = String, Path, description = "The endpoint ID")),
            request_body(content((String = "*/*")), description = "Any body"),
            responses(
                (status = 200, description = "Stored", body = Captured),
                (status = 404, description = "No enabled endpoint has this ID", body = ErrorBody),
                (status = 413, description = "The body is over 256 KiB"),
                (status = 429, description = "The endpoint stored 300 requests in the past hour; retry after a minute", body = ErrorBody),
            )
        )]
        #[expect(dead_code, reason = "exists for its utoipa docs")]
        const fn $name() {}
    )+};
}

capture_docs! {
    get => capture_get_docs, "webhooks.public.capture_get";
    post => capture_post_docs, "webhooks.public.capture_post";
    put => capture_put_docs, "webhooks.public.capture_put";
    patch => capture_patch_docs, "webhooks.public.capture_patch";
    delete => capture_delete_docs, "webhooks.public.capture_delete";
}

/// The docs for the public routes, relative to the nest.
#[derive(OpenApi)]
#[openapi(
    paths(
        capture_get_docs,
        capture_post_docs,
        capture_put_docs,
        capture_patch_docs,
        capture_delete_docs
    ),
    components(schemas(Captured, ErrorBody))
)]
pub(crate) struct CaptureDocs;

fn request_id(headers: &HeaderMap) -> String {
    headers.get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or("-").to_owned()
}

#[expect(clippy::too_many_arguments, reason = "one argument per axum extractor")]
async fn capture(
    Extension(platform): Extension<Platform>,
    Path(endpoint_id): Path<String>,
    method: Method,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let request_id = request_id(&headers);
    let incoming = Incoming { method, query, headers: &headers, body };
    match take(&platform, &endpoint_id, incoming).await {
        Ok(()) => Json(Captured { ok: true }).into_response(),
        Err(Refused::TooLarge) => StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        Err(Refused::Api(err)) => {
            err.log(&request_id);
            err.into_response()
        }
    }
}

/// Why a request wasn't stored.
#[derive(Debug)]
enum Refused {
    /// Core's errors have no 413 kind, so this answer has no body.
    TooLarge,
    Api(ApiError),
}

impl<E: Into<ApiError>> From<E> for Refused {
    fn from(err: E) -> Self {
        Self::Api(err.into())
    }
}

/// The parts of a request that get stored.
struct Incoming<'a> {
    method: Method,
    query: Option<String>,
    headers: &'a HeaderMap,
    body: Body,
}

async fn take(
    platform: &Platform,
    endpoint_id: &str,
    incoming: Incoming<'_>,
) -> Result<(), Refused> {
    let Incoming { method, query, headers, body } = incoming;
    let Ok(id) = endpoint_id.parse::<EndpointId>() else { return Err(ApiError::NotFound.into()) };
    let endpoint = store::find_public(platform.db().read(), id).await?.ok_or(ApiError::NotFound)?;
    // An endpoint whose workspace turned webhooks off captures nothing.
    if !platform
        .feature_enabled(endpoint.workspace_id, "webhooks")
        .await
        .map_err(ApiError::Internal)?
    {
        return Err(ApiError::NotFound.into());
    }
    let body = axum::body::to_bytes(body, MAX_BODY_BYTES).await.map_err(|_| Refused::TooLarge)?;
    let row = RequestRow {
        id: RequestId::new(),
        workspace_id: endpoint.workspace_id,
        endpoint_id: endpoint.id,
        method: method.as_str().to_owned(),
        query: query.filter(|q| !q.is_empty()),
        headers: serde_json::to_string(&kept_headers(headers)).map_err(anyhow::Error::from)?,
        content_type: headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .filter(|v| v.len() <= HEADER_CHARS)
            .map(str::to_owned),
        body: body.to_vec(),
        delivery: None,
        received_at: platform.now(),
    };
    Ok(store_request(platform, &endpoint, &row, headers).await?)
}

async fn store_request(
    platform: &Platform,
    endpoint: &EndpointRow,
    row: &RequestRow,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let mut tx = platform.db().write().await?.with_request_id(request_id(headers));
    // The body upload can be slow; the endpoint may have been disabled or
    // deleted since the first check.
    if store::find_public(&mut *tx, endpoint.id).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    let hour_ago = row.received_at.minus(Duration::from_hours(1));
    let recent = store::recent(&mut *tx, endpoint.workspace_id, endpoint.id, hour_ago).await?;
    if recent >= STORED_PER_HOUR {
        return Err(ApiError::RateLimited { retry_after_secs: 60 });
    }
    store::insert_request(&mut *tx, row).await?;
    store::prune(&mut *tx, endpoint.workspace_id, endpoint.id, KEEP_PER_ENDPOINT).await?;
    let event = RequestReceived {
        workspace_id: endpoint.workspace_id,
        endpoint_id: endpoint.id,
        request_id: row.id,
    };
    platform.bus().publish(&mut tx, &event).await?;
    if endpoint.forward_url.is_some() {
        let job = Forward {
            workspace_id: endpoint.workspace_id,
            endpoint_id: endpoint.id,
            request_id: row.id,
        };
        platform.bus().enqueue(&mut tx, &job, Enqueue::default()).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The headers worth keeping, as lowercase `[name, value]` pairs: names in
/// the order they first arrived, a repeated name's values together.
pub(crate) fn kept_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter(|(name, _)| !DROPPED_HEADERS.contains(&name.as_str()))
        .filter_map(|(name, value)| {
            let value = value.to_str().ok().filter(|v| v.len() <= HEADER_CHARS)?;
            Some((name.as_str().to_owned(), value.to_owned()))
        })
        .take(MAX_HEADERS)
        .collect()
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderName, HeaderValue};

    use super::*;

    #[test]
    fn credentials_and_hop_headers_are_dropped() {
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("authorization", "Bearer secret"),
            ("cookie", "session=1"),
            ("proxy-authorization", "Basic x"),
            ("host", "example.com"),
            ("content-length", "2"),
            ("content-type", "application/json"),
            ("stripe-signature", "t=1,v1=abc"),
        ] {
            headers.insert(HeaderName::from_static(name), HeaderValue::from_static(value));
        }
        assert_eq!(
            kept_headers(&headers),
            vec![
                ("content-type".to_owned(), "application/json".to_owned()),
                ("stripe-signature".to_owned(), "t=1,v1=abc".to_owned()),
            ],
            "kept"
        );
    }

    #[test]
    fn long_values_are_dropped_and_headers_capped() {
        let mut headers = HeaderMap::new();
        let long = "x".repeat(5000);
        headers.insert("x-long", HeaderValue::from_str(&long).unwrap());
        for i in 0..150 {
            headers.append(
                HeaderName::from_bytes(format!("x-h{i}").as_bytes()).unwrap(),
                HeaderValue::from_static("v"),
            );
        }
        let kept = kept_headers(&headers);
        assert_eq!(kept.len(), MAX_HEADERS, "count capped");
        assert!(kept.iter().all(|(name, _)| name != "x-long"), "long value dropped");
    }
}
