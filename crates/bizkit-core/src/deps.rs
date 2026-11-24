//! Side effects the platform reaches through traits, so tests can swap in
//! fakes: time, mail, outbound HTTP and blob storage.

mod fs_blobs;
mod log_mailer;
mod reqwest_http;
mod smtp_mailer;
mod system_clock;

use std::fmt;
use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use url::Url;

pub use self::fs_blobs::FsBlobs;
pub use self::log_mailer::LogMailer;
pub use self::reqwest_http::ReqwestHttp;
pub use self::smtp_mailer::SmtpMailer;
pub use self::system_clock::SystemClock;
use crate::config::Config;
use crate::time::UnixMillis;

/// The platform's side effects.
#[derive(Debug, Clone)]
pub struct Deps {
    /// The current time.
    pub clock: Arc<dyn Clock>,
    /// Outgoing mail.
    pub mailer: Arc<dyn Mailer>,
    /// Outbound HTTP to public addresses only; see [`HttpClient`].
    pub http: Arc<dyn HttpClient>,
    /// Blob storage.
    pub blobs: Arc<dyn BlobStore>,
}

impl Deps {
    /// The production implementations for `config`. `data_dir` must be set.
    ///
    /// # Errors
    ///
    /// When `data_dir` is unset or the mailer or HTTP client can't be built.
    pub fn from_config(config: &Config) -> anyhow::Result<Self> {
        let data_dir = config.data_dir.as_deref().context("data_dir is not set")?;
        let mailer: Arc<dyn Mailer> = match &config.mail.smtp_url {
            Some(url) => Arc::new(SmtpMailer::new(url, &config.mail)?),
            None => Arc::new(LogMailer::new(&config.mail.from)),
        };
        Ok(Self {
            clock: Arc::new(SystemClock),
            mailer,
            http: Arc::new(ReqwestHttp::new()?),
            blobs: Arc::new(FsBlobs::new(data_dir.join("blobs"))),
        })
    }
}

/// The current time.
pub trait Clock: Send + Sync + fmt::Debug {
    /// Now, in unix milliseconds.
    fn now(&self) -> UnixMillis;
}

/// One outgoing message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mail {
    /// The recipient address.
    pub to: String,
    /// The subject line.
    pub subject: String,
    /// The plain-text body.
    pub text: String,
    /// An optional HTML body.
    #[serde(default)]
    pub html: Option<String>,
    /// Where a list mail's recipient unsubscribes. Sent as `List-Unsubscribe`
    /// with `List-Unsubscribe-Post: List-Unsubscribe=One-Click` (RFC 8058).
    #[serde(default)]
    pub unsubscribe: Option<Url>,
}

/// Sends mail.
#[async_trait]
pub trait Mailer: Send + Sync + fmt::Debug {
    /// Delivers `mail` or returns why it couldn't.
    async fn send(&self, mail: &Mail) -> anyhow::Result<()>;
}

/// Sends outbound HTTP requests to user-supplied URLs. The production
/// client ([`ReqwestHttp`]) reaches public addresses only, follows no
/// redirects, times out after 10 s and reads at most 1 MiB.
#[async_trait]
pub trait HttpClient: Send + Sync + fmt::Debug {
    /// Sends `req` and reads the whole response body, up to the client's cap.
    async fn send(&self, req: http::Request<Bytes>) -> anyhow::Result<http::Response<Bytes>>;
}

/// A key in the blob store: `/`-separated segments, none empty or `..`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlobKey(String);

/// Why a string is not a valid [`BlobKey`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid blob key {0:?}: segments must be non-empty and not `.` or `..`")]
pub struct BlobKeyError(String);

impl BlobKey {
    /// Parses a key, rejecting traversal, leading `/` and empty segments.
    ///
    /// # Errors
    ///
    /// When `key` is empty or has an empty, `.` or `..` segment.
    pub fn new(key: impl Into<String>) -> Result<Self, BlobKeyError> {
        let key = key.into();
        let valid = !key.is_empty()
            && key
                .split('/')
                .all(|seg| !seg.is_empty() && seg != "." && seg != ".." && !seg.contains('\\'));
        if valid { Ok(Self(key)) } else { Err(BlobKeyError(key)) }
    }

    /// The key text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlobKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A stored blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    /// The content.
    pub body: Bytes,
    /// The MIME type given at `put`.
    pub content_type: String,
}

/// Stores opaque blobs by key.
#[async_trait]
pub trait BlobStore: Send + Sync + fmt::Debug {
    /// Writes `body` at `key`, replacing any blob there.
    async fn put(&self, key: &BlobKey, body: Bytes, content_type: &str) -> anyhow::Result<()>;
    /// Reads the blob at `key`, or `None`.
    async fn get(&self, key: &BlobKey) -> anyhow::Result<Option<Blob>>;
    /// Deletes the blob at `key`. Deleting a missing key is not an error.
    async fn delete(&self, key: &BlobKey) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_keys_reject_traversal() {
        for bad in ["", "/a", "a/", "a//b", "../a", "a/../b", "a/./b", "a\\b"] {
            assert!(BlobKey::new(bad).is_err(), "{bad:?} should be rejected");
        }
        for good in ["a", "a/b", "forms/0190/upload.pdf", "a..b"] {
            assert!(BlobKey::new(good).is_ok(), "{good:?} should be accepted");
        }
    }
}
