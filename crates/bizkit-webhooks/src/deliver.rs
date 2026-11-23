//! Sending a stored request on to a URL: the `webhooks.forward` job after
//! capture, and replay from the API. Both record the latest [`Delivery`].

use std::time::{Duration, Instant};

use anyhow::Context as _;
use bizkit_core::db::WriteTx;
use bizkit_core::deps::HttpClient;
use bizkit_core::ids::WorkspaceId;
use bizkit_core::jobs::{Job, JobCtx, JobHandler, Lane};
use bizkit_core::time::UnixMillis;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::api::{Delivery, EndpointId, RequestId};
use crate::store::{self, RequestRow};

/// The longest error text kept on a delivery.
const ERROR_CHARS: usize = 300;

/// The worker lane forwards use, so slow targets never hold up mail.
pub(crate) const WEBHOOKS: Lane = Lane { name: "webhooks", concurrency: 2 };

/// Sends a stored request to its endpoint's forward URL. It skips when the
/// request was pruned, or the endpoint was deleted, disabled or stopped
/// forwarding since.
#[expect(clippy::struct_field_names, reason = "the IDs are named for what they identify")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Forward {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) endpoint_id: EndpointId,
    pub(crate) request_id: RequestId,
}

impl Job for Forward {
    const KIND: &'static str = "webhooks.forward";
    const MAX_ATTEMPTS: u32 = 3;
    const LANE: Lane = WEBHOOKS;
    const TIMEOUT: Duration = Duration::from_secs(30);
}

/// Whether a failed delivery is worth another attempt: no answer, or a 5xx.
fn retryable(delivery: &Delivery) -> bool {
    delivery.status.is_none_or(|status| status >= 500)
}

fn capped(text: &str) -> String {
    text.chars().take(ERROR_CHARS).collect()
}

/// Sends `request` to `url` with its stored method, headers and body.
/// Never fails: transport errors and non-2xx answers land in
/// [`Delivery::error`].
pub(crate) async fn deliver(
    http: &dyn HttpClient,
    at: UnixMillis,
    request: &RequestRow,
    url: &Url,
) -> Delivery {
    let started = Instant::now();
    let result = send(http, request, url).await;
    let took_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let (status, error) = match result {
        Ok(status) if (200..300).contains(&status) => (Some(status), None),
        Ok(status) => (Some(status), Some(format!("the target answered {status}"))),
        Err(err) => (None, Some(capped(&format!("{err:#}")))),
    };
    Delivery { url: url.to_string(), status, error, took_ms, at }
}

async fn send(http: &dyn HttpClient, request: &RequestRow, url: &Url) -> anyhow::Result<u16> {
    bizkit_core::net::check_url(url)?;
    let headers: Vec<(String, String)> =
        serde_json::from_str(&request.headers).context("decoding the stored headers")?;
    let mut builder = http::Request::builder()
        .method(http::Method::from_bytes(request.method.as_bytes()).context("the stored method")?)
        .uri(url.as_str());
    for (name, value) in &headers {
        builder = builder.header(name, value);
    }
    let req = builder.body(Bytes::from(request.body.clone())).context("building the request")?;
    let res = http.send(req).await?;
    Ok(res.status().as_u16())
}

/// Stores `delivery` as the request's latest. A request deleted meanwhile
/// is not an error.
pub(crate) async fn record(
    tx: &mut WriteTx,
    workspace_id: WorkspaceId,
    request_id: RequestId,
    delivery: &Delivery,
) -> anyhow::Result<()> {
    let text = serde_json::to_string(delivery).context("encoding the delivery")?;
    store::set_delivery(&mut **tx, workspace_id, request_id, &text).await?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ForwardHandler;

impl JobHandler<Forward> for ForwardHandler {
    async fn handle(&self, cx: JobCtx, job: Forward) -> anyhow::Result<()> {
        let platform = &cx.platform;
        let db = platform.db().read();
        let endpoint = store::find_endpoint(db, job.workspace_id, job.endpoint_id)
            .await
            .context("loading the endpoint")?;
        let Some(url) = endpoint
            .filter(|e| e.enabled)
            .and_then(|e| e.forward_url)
            .and_then(|url| Url::parse(&url).ok())
        else {
            return Ok(());
        };
        let Some(request) =
            store::find_request(db, job.workspace_id, job.endpoint_id, job.request_id)
                .await
                .context("loading the request")?
        else {
            return Ok(());
        };
        let delivery = deliver(platform.deps().http.as_ref(), platform.now(), &request, &url).await;
        let mut tx = platform.db().write().await?;
        record(&mut tx, job.workspace_id, job.request_id, &delivery).await?;
        tx.commit().await?;
        match &delivery.error {
            Some(error) if retryable(&delivery) => anyhow::bail!("forwarding failed: {error}"),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delivery(status: Option<u16>, error: Option<&str>) -> Delivery {
        Delivery {
            url: "https://example.com".to_owned(),
            status,
            error: error.map(str::to_owned),
            took_ms: 1,
            at: UnixMillis(0),
        }
    }

    #[test]
    fn only_missing_answers_and_server_errors_retry() {
        assert!(retryable(&delivery(None, Some("timed out"))), "no answer");
        assert!(retryable(&delivery(Some(502), Some("the target answered 502"))), "5xx");
        assert!(!retryable(&delivery(Some(404), Some("the target answered 404"))), "4xx");
    }
}
