use anyhow::Context as _;
use async_trait::async_trait;
use bytes::Bytes;

use super::HttpClient;

/// Outbound HTTP through reqwest with rustls. It has no SSRF guard: modules
/// that fetch user-supplied URLs must add one.
#[derive(Debug, Clone)]
pub struct ReqwestHttp {
    client: reqwest::Client,
}

impl ReqwestHttp {
    /// A client with a 30 s timeout and the bizkit user agent.
    ///
    /// # Errors
    ///
    /// When the TLS backend fails to initialize.
    pub fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("bizkit/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("building the HTTP client")?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpClient for ReqwestHttp {
    async fn send(&self, req: http::Request<Bytes>) -> anyhow::Result<http::Response<Bytes>> {
        let uri = req.uri().to_string();
        let req = reqwest::Request::try_from(req).context("converting the request")?;
        let resp = self.client.execute(req).await.with_context(|| format!("requesting {uri}"))?;
        let mut builder = http::Response::builder().status(resp.status()).version(resp.version());
        if let Some(headers) = builder.headers_mut() {
            headers.extend(resp.headers().iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        let body = resp.bytes().await.with_context(|| format!("reading the body of {uri}"))?;
        builder.body(body).context("building the response")
    }
}
