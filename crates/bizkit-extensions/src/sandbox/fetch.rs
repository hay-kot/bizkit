//! `ctx.fetch`: carries a request from a sandbox thread to the main tokio
//! runtime, where `Deps.http` sends it.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use bizkit_core::deps::HttpClient;
use bizkit_core::net;
use bytes::Bytes;
use serde::Deserialize;
use serde_json::json;
use url::Url;

/// The most fetches one run can make.
pub const MAX_FETCHES: u32 = 5;
/// The longest one fetch can take.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
/// The largest request body a script can send.
const MAX_REQUEST_BYTES: usize = 256 << 10;

/// Sends a run's fetches. Each run gets its own, so the call count and the
/// deadline are per run.
#[derive(Debug, Clone)]
pub struct FetchBridge {
    http: Arc<dyn HttpClient>,
    handle: tokio::runtime::Handle,
    deadline: Instant,
    calls: Arc<AtomicU32>,
}

#[derive(Debug, Default, Deserialize)]
struct Init {
    method: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    body: Option<String>,
}

impl FetchBridge {
    /// A bridge whose fetches all end by `deadline`, the run's interrupt
    /// budget.
    #[must_use]
    pub fn new(
        http: Arc<dyn HttpClient>,
        handle: tokio::runtime::Handle,
        deadline: Instant,
    ) -> Self {
        Self { http, handle, deadline, calls: Arc::new(AtomicU32::new(0)) }
    }

    /// Sends one request and answers with JSON for the prelude:
    /// `{status, headers, body}` or `{error}`. Errors name the method and
    /// host, never the full URL, which can hold a secret.
    pub(super) fn fetch(&self, url: &str, init: &str) -> String {
        match self.send(url, init) {
            Ok(reply) => reply.to_string(),
            Err(error) => json!({ "error": error }).to_string(),
        }
    }

    fn send(&self, url: &str, init: &str) -> Result<serde_json::Value, String> {
        if self.calls.fetch_add(1, Ordering::SeqCst) >= MAX_FETCHES {
            return Err(format!("a run can fetch at most {MAX_FETCHES} times"));
        }
        let url =
            Url::parse(url).map_err(|_| "fetch needs an absolute http or https URL".to_owned())?;
        let host = url.host_str().unwrap_or_default().to_owned();
        net::check_url(&url).map_err(|err| err.to_string())?;
        if host == "localhost" || host.ends_with(".localhost") {
            return Err(format!("{host} is not a public address"));
        }
        let init: Init =
            serde_json::from_str(init).map_err(|err| format!("fetch options: {err}"))?;
        let method = init.method.as_deref().unwrap_or("GET").to_ascii_uppercase();
        let label = format!("{method} {host}");
        let body = init.body.unwrap_or_default();
        if body.len() > MAX_REQUEST_BYTES {
            return Err(format!("{label}: the body is over {} KiB", MAX_REQUEST_BYTES >> 10));
        }
        let mut request = http::Request::builder().method(method.as_str()).uri(url.as_str());
        for (name, value) in &init.headers {
            request = request.header(name, value);
        }
        let request = request.body(Bytes::from(body)).map_err(|err| format!("{label}: {err}"))?;
        let left = self.deadline.saturating_duration_since(Instant::now()).min(FETCH_TIMEOUT);
        if left.is_zero() {
            return Err(format!("{label}: no time left in the run's budget"));
        }
        let http = Arc::clone(&self.http);
        // Built inside the block: a tokio timer made off the runtime panics, and
        // a panic here would abort, since it can't unwind through QuickJS.
        let sent = self
            .handle
            .block_on(async move { tokio::time::timeout(left, http.send(request)).await });
        let response = match sent {
            Err(_) => return Err(format!("{label}: timed out after {left:?}")),
            // The HTTP client strips URLs from its errors, and the run
            // record redacts secrets, so the cause is safe to show.
            Ok(Err(err)) => return Err(format!("{label}: {err:#}")),
            Ok(Ok(response)) => response,
        };
        let headers: BTreeMap<&str, &str> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| Some((name.as_str(), value.to_str().ok()?)))
            .collect();
        Ok(json!({
            "status": response.status().as_u16(),
            "headers": headers,
            "body": String::from_utf8_lossy(response.body()),
        }))
    }
}
