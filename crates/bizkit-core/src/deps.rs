//! Side effects the platform reaches through traits, so tests can swap in
//! fakes: time, mail, outbound HTTP and blob storage.

mod fs_blobs;
mod log_mailer;
mod mailgun_mailer;
mod reqwest_http;
mod smtp_mailer;
mod system_clock;
mod throttle;

use std::fmt;
use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use url::Url;

pub use self::fs_blobs::FsBlobs;
pub use self::log_mailer::LogMailer;
pub use self::mailgun_mailer::{MailgunMailer, verify_mailgun_signature};
pub use self::reqwest_http::ReqwestHttp;
pub use self::smtp_mailer::SmtpMailer;
pub use self::system_clock::SystemClock;
pub use self::throttle::ThrottledMailer;
use crate::config::{Config, MailProvider};
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
    /// When `data_dir` is unset, the mail provider settings don't fit
    /// together, or the mailer or HTTP client can't be built.
    pub fn from_config(config: &Config) -> anyhow::Result<Self> {
        let data_dir = config.data_dir.as_deref().context("data_dir is not set")?;
        let http: Arc<dyn HttpClient> = Arc::new(ReqwestHttp::new()?);
        let mailer = mailer(config, &http)?;
        Ok(Self {
            clock: Arc::new(SystemClock),
            mailer,
            http,
            blobs: Arc::new(FsBlobs::new(data_dir.join("blobs"))),
        })
    }
}

/// Runs after the command-line overrides, so it sees the final `smtp_url`.
fn mailer(config: &Config, http: &Arc<dyn HttpClient>) -> anyhow::Result<Arc<dyn Mailer>> {
    let mail = &config.mail;
    let mailer: Arc<dyn Mailer> = match mail.provider {
        MailProvider::Smtp => match &mail.smtp_url {
            Some(url) => Arc::new(SmtpMailer::new(url, mail)?),
            None => Arc::new(LogMailer::new(&mail.from)),
        },
        MailProvider::Mailgun => {
            anyhow::ensure!(
                mail.smtp_url.is_none(),
                "mail.provider is mailgun, so mail.smtp_url (or --smtp-url) must be unset"
            );
            let mailgun = mail
                .mailgun
                .as_ref()
                .context("mail.provider is mailgun but mail.mailgun is unset")?;
            Arc::new(MailgunMailer::new(Arc::clone(http), mailgun, &mail.from)?)
        }
    };
    Ok(match mail.rate_per_second {
        Some(rate) => Arc::new(ThrottledMailer::new(mailer, rate)),
        None => mailer,
    })
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
    /// The local part of the `Message-ID`; the mailer adds `@<from domain>`.
    /// Providers also get it as metadata (Mailgun `v:bizkit_id`), so their
    /// events find the mail again. `None` lets the mailer pick an ID.
    #[serde(default)]
    pub id: Option<String>,
}

/// Why a mail wasn't sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MailError {
    /// Sending this mail again won't help: an SMTP 5xx other than 530 and
    /// 535, a Mailgun 400 or 413, or a recipient that doesn't parse.
    #[error("permanent: {0}")]
    Permanent(String),
    /// Sending again later may work: an SMTP 4xx, 530 or 535 (credentials),
    /// a network error, or a Mailgun 401, 403, 404, 429 or 5xx.
    #[error("{0}")]
    Transient(String),
}

impl MailError {
    /// Whether sending the mail again can't help.
    #[must_use]
    pub const fn is_permanent(&self) -> bool {
        matches!(self, Self::Permanent(_))
    }
}

/// Sends mail.
#[async_trait]
pub trait Mailer: Send + Sync + fmt::Debug {
    /// Delivers `mail`. Returns the provider's message ID (Mailgun's `id`) or
    /// the `Message-ID` the mailer set, without angle brackets, or `None`
    /// when nothing was sent (the log mailer).
    ///
    /// # Errors
    ///
    /// [`MailError::Permanent`] when a retry can't succeed, else
    /// [`MailError::Transient`].
    async fn send(&self, mail: &Mail) -> Result<Option<String>, MailError>;
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
    fn mailgun_refuses_an_smtp_url_set_by_flag() {
        let mut config = Config::default();
        config.mail.provider = MailProvider::Mailgun;
        let http: Arc<dyn HttpClient> = Arc::new(ReqwestHttp::new().unwrap());
        let no_section = mailer(&config, &http).unwrap_err();
        assert!(no_section.to_string().contains("mail.mailgun is unset"), "{no_section}");
        config.mail.smtp_url = Some("smtp://127.0.0.1:1025".parse().unwrap());
        let with_smtp = mailer(&config, &http).unwrap_err();
        assert!(with_smtp.to_string().contains("smtp_url"), "{with_smtp}");
    }

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
