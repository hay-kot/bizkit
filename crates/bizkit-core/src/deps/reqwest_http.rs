use std::time::Duration;

use anyhow::{Context as _, bail};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};

use super::HttpClient;
use crate::net::{PublicOnlyResolver, check_url};

/// The largest response body read.
const MAX_BODY: usize = 1024 * 1024;

/// Outbound HTTP through reqwest with rustls, for user-supplied URLs.
///
/// It only reaches public addresses, follows no redirects, ignores proxy env
/// vars (a proxy would resolve names itself and skip the guard), stops after
/// 10 s and reads at most 1 MiB.
#[derive(Debug, Clone)]
pub struct ReqwestHttp {
    client: reqwest::Client,
}

impl ReqwestHttp {
    /// The public-only client with the bizkit user agent.
    ///
    /// # Errors
    ///
    /// When the TLS backend fails to initialize.
    pub fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("bizkit/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .dns_resolver(std::sync::Arc::new(PublicOnlyResolver))
            .build()
            .context("building the HTTP client")?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpClient for ReqwestHttp {
    async fn send(&self, req: http::Request<Bytes>) -> anyhow::Result<http::Response<Bytes>> {
        let url = url::Url::parse(&req.uri().to_string()).context("parsing the request URL")?;
        check_url(&url)?;
        // Errors name the host only: paths and queries can carry tokens.
        let host = url.host_str().unwrap_or_default().to_owned();
        let req = reqwest::Request::try_from(req).context("converting the request")?;
        let mut resp = self
            .client
            .execute(req)
            .await
            .map_err(reqwest::Error::without_url)
            .with_context(|| format!("requesting {host}"))?;
        let mut builder = http::Response::builder().status(resp.status()).version(resp.version());
        if let Some(headers) = builder.headers_mut() {
            headers.extend(resp.headers().iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        let mut body = BytesMut::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(reqwest::Error::without_url)
            .with_context(|| format!("reading the response from {host}"))?
        {
            if body.len() + chunk.len() > MAX_BODY {
                bail!("the response from {host} is larger than {MAX_BODY} bytes");
            }
            body.extend_from_slice(&chunk);
        }
        builder.body(body.freeze()).context("building the response")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::net::EgressError;

    #[tokio::test]
    async fn refuses_loopback_before_connecting() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let connected = Arc::new(AtomicBool::new(false));
        let accept = tokio::spawn({
            let connected = Arc::clone(&connected);
            async move {
                if listener.accept().await.is_ok() {
                    connected.store(true, Ordering::SeqCst);
                }
            }
        });
        let http = ReqwestHttp::new().unwrap();
        for url in [format!("http://127.0.0.1:{port}/"), format!("http://localhost:{port}/")] {
            let req = http::Request::get(url.as_str()).body(Bytes::new()).unwrap();
            let err = http.send(req).await.expect_err("loopback must be refused");
            let blocked = err.chain().any(|cause| {
                cause.downcast_ref::<EgressError>().is_some()
                    || cause.to_string().contains("is not a public address")
            });
            assert!(blocked, "{url}: {err:#}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        accept.abort();
        assert!(!connected.load(Ordering::SeqCst), "nothing connected to the listener");
    }
}
