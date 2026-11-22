//! In-memory fakes for the platform's side effects.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use bizkit_core::deps::{Blob, BlobKey, BlobStore, Clock, HttpClient, Mail, Mailer};
use bizkit_core::time::UnixMillis;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panicking test poisons the lock; the data is still fine to read.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A clock that moves only when told to. Starts at 2026-01-01T00:00:00Z.
#[derive(Debug, Clone)]
pub struct FakeClock {
    now: Arc<AtomicI64>,
}

impl Default for FakeClock {
    fn default() -> Self {
        Self { now: Arc::new(AtomicI64::new(1_767_225_600_000)) }
    }
}

impl FakeClock {
    /// Sets the time.
    pub fn set(&self, at: UnixMillis) {
        self.now.store(at.0, Ordering::SeqCst);
    }

    /// Moves the time forward.
    pub fn advance(&self, by: Duration) {
        let millis = i64::try_from(by.as_millis()).unwrap_or(i64::MAX);
        self.now.fetch_add(millis, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.now.load(Ordering::SeqCst))
    }
}

/// Records mail instead of sending it.
#[derive(Debug, Clone, Default)]
pub struct FakeMailer {
    sent: Arc<Mutex<Vec<Mail>>>,
    fail_next: Arc<AtomicU32>,
}

impl FakeMailer {
    /// Every mail sent so far.
    #[must_use]
    pub fn sent(&self) -> Vec<Mail> {
        lock(&self.sent).clone()
    }

    /// Every mail sent so far, clearing the record.
    #[must_use]
    pub fn take(&self) -> Vec<Mail> {
        std::mem::take(&mut *lock(&self.sent))
    }

    /// The newest mail to `email`.
    #[must_use]
    pub fn last_to(&self, email: &str) -> Option<Mail> {
        lock(&self.sent).iter().rev().find(|mail| mail.to == email).cloned()
    }

    /// Makes the next `n` sends fail.
    pub fn fail_next(&self, n: u32) {
        self.fail_next.store(n, Ordering::SeqCst);
    }
}

#[async_trait]
impl Mailer for FakeMailer {
    async fn send(&self, mail: &Mail) -> anyhow::Result<()> {
        let failing = self
            .fail_next
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok();
        if failing {
            anyhow::bail!("fake mailer told to fail");
        }
        lock(&self.sent).push(mail.clone());
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Canned {
    method: Method,
    url_prefix: String,
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

/// Answers outbound HTTP from canned responses. An unmatched request is an
/// error, so tests never reach the network.
#[derive(Debug, Clone, Default)]
pub struct FakeHttp {
    canned: Arc<Mutex<Vec<Canned>>>,
    requests: Arc<Mutex<Vec<(Method, String, Bytes)>>>,
}

impl FakeHttp {
    /// Answers `method` requests whose URL starts with `url_prefix`. Later
    /// registrations win.
    pub fn respond(
        &self,
        method: Method,
        url_prefix: &str,
        status: StatusCode,
        body: impl Into<Bytes>,
    ) {
        lock(&self.canned).push(Canned {
            method,
            url_prefix: url_prefix.to_owned(),
            status,
            headers: HeaderMap::new(),
            body: body.into(),
        });
    }

    /// Every request so far: method, URL and body.
    #[must_use]
    pub fn requests(&self) -> Vec<(Method, String, Bytes)> {
        lock(&self.requests).clone()
    }
}

#[async_trait]
impl HttpClient for FakeHttp {
    async fn send(&self, req: http::Request<Bytes>) -> anyhow::Result<http::Response<Bytes>> {
        let url = req.uri().to_string();
        lock(&self.requests).push((req.method().clone(), url.clone(), req.body().clone()));
        let canned = lock(&self.canned)
            .iter()
            .rev()
            .find(|c| c.method == req.method() && url.starts_with(&c.url_prefix))
            .cloned();
        let Some(canned) = canned else {
            anyhow::bail!("FakeHttp has no response for {} {url}", req.method());
        };
        let mut response = http::Response::builder().status(canned.status).body(canned.body)?;
        *response.headers_mut() = canned.headers;
        Ok(response)
    }
}

/// Blobs in memory.
#[derive(Debug, Clone, Default)]
pub struct MemBlobs {
    blobs: Arc<Mutex<BTreeMap<BlobKey, Blob>>>,
}

impl MemBlobs {
    /// Every stored key.
    #[must_use]
    pub fn keys(&self) -> Vec<BlobKey> {
        lock(&self.blobs).keys().cloned().collect()
    }
}

#[async_trait]
impl BlobStore for MemBlobs {
    async fn put(&self, key: &BlobKey, body: Bytes, content_type: &str) -> anyhow::Result<()> {
        lock(&self.blobs).insert(key.clone(), Blob { body, content_type: content_type.to_owned() });
        Ok(())
    }

    async fn get(&self, key: &BlobKey) -> anyhow::Result<Option<Blob>> {
        Ok(lock(&self.blobs).get(key).cloned())
    }

    async fn delete(&self, key: &BlobKey) -> anyhow::Result<()> {
        lock(&self.blobs).remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mailer_fails_on_request() {
        let mailer = FakeMailer::default();
        mailer.fail_next(1);
        let mail = Mail {
            to: "a@example.com".to_owned(),
            subject: "s".to_owned(),
            text: "t".to_owned(),
            html: None,
        };
        assert!(mailer.send(&mail).await.is_err(), "first send fails");
        assert!(mailer.send(&mail).await.is_ok(), "second send works");
        assert_eq!(mailer.last_to("a@example.com"), Some(mail), "recorded");
    }

    #[tokio::test]
    async fn http_refuses_unmatched_requests() {
        let http = FakeHttp::default();
        http.respond(Method::GET, "https://api.example.com/", StatusCode::OK, "hi");
        let ok = http::Request::get("https://api.example.com/x").body(Bytes::new()).unwrap();
        assert_eq!(http.send(ok).await.unwrap().body().as_ref(), b"hi", "canned body");
        let other = http::Request::get("https://other.example.com/").body(Bytes::new()).unwrap();
        assert!(http.send(other).await.is_err(), "unmatched is an error");
        assert_eq!(http.requests().len(), 2, "both requests recorded");
    }
}
