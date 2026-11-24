use std::env;
use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bytes::Bytes;
use hmac::{KeyInit as _, Mac as _};
use http::StatusCode;
use lettre::message::Mailbox;
use serde::Deserialize;
use url::Url;

use super::{HttpClient, Mail, MailError, Mailer};
use crate::config::{MailgunConfig, MailgunRegion};

/// Sends mail through Mailgun's HTTP API (`POST /v3/<domain>/messages`).
#[derive(Debug, Clone)]
pub struct MailgunMailer {
    http: Arc<dyn HttpClient>,
    endpoint: Url,
    authorization: String,
    from: Mailbox,
}

impl MailgunMailer {
    /// A mailer for `config`'s domain and region. The API key comes from
    /// `config.api_key_env`, so it never sits in the config file.
    ///
    /// # Errors
    ///
    /// When `from` doesn't parse or the key variable is unset.
    pub fn new(
        http: Arc<dyn HttpClient>,
        config: &MailgunConfig,
        from: &str,
    ) -> anyhow::Result<Self> {
        let key = env::var(&config.api_key_env)
            .with_context(|| format!("reading the Mailgun API key from ${}", config.api_key_env))?;
        Self::with_key(http, config, from, &key)
    }

    fn with_key(
        http: Arc<dyn HttpClient>,
        config: &MailgunConfig,
        from: &str,
        key: &str,
    ) -> anyhow::Result<Self> {
        let from: Mailbox = from.parse().with_context(|| format!("parsing mail.from {from:?}"))?;
        let host = match config.region {
            MailgunRegion::Us => "api.mailgun.net",
            MailgunRegion::Eu => "api.eu.mailgun.net",
        };
        let mut endpoint =
            Url::parse(&format!("https://{host}/v3/")).context("building the Mailgun endpoint")?;
        endpoint
            .path_segments_mut()
            .map_err(|()| anyhow::anyhow!("the Mailgun endpoint has no path"))?
            .pop_if_empty()
            .push(&config.domain)
            .push("messages");
        let authorization = format!("Basic {}", STANDARD.encode(format!("api:{key}")));
        Ok(Self { http, endpoint, authorization, from })
    }

    fn form(&self, mail: &Mail) -> String {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("from", &self.from.to_string())
            .append_pair("to", &mail.to)
            .append_pair("subject", &mail.subject)
            .append_pair("text", &mail.text);
        if let Some(html) = &mail.html {
            form.append_pair("html", html);
        }
        if let Some(id) = &mail.id {
            form.append_pair("h:Message-Id", &format!("<{id}@{}>", self.from.email.domain()))
                .append_pair("v:bizkit_id", id);
        }
        if let Some(url) = &mail.unsubscribe {
            form.append_pair("h:List-Unsubscribe", &format!("<{url}>"))
                .append_pair("h:List-Unsubscribe-Post", "List-Unsubscribe=One-Click");
        }
        form.finish()
    }
}

#[derive(Deserialize)]
struct Accepted {
    id: String,
}

#[async_trait]
impl Mailer for MailgunMailer {
    async fn send(&self, mail: &Mail) -> Result<Option<String>, MailError> {
        let request = http::Request::post(self.endpoint.as_str())
            .header(http::header::AUTHORIZATION, &self.authorization)
            .header(http::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Bytes::from(self.form(mail)))
            .map_err(|err| MailError::Permanent(format!("building the Mailgun request: {err}")))?;
        let response = self
            .http
            .send(request)
            .await
            .map_err(|err| MailError::Transient(format!("sending through Mailgun: {err:#}")))?;
        let status = response.status();
        if status == StatusCode::OK {
            let accepted: Accepted = serde_json::from_slice(response.body())
                .map_err(|err| MailError::Transient(format!("reading Mailgun's answer: {err}")))?;
            return Ok(Some(accepted.id.trim_matches(['<', '>']).to_owned()));
        }
        let text = format!(
            "Mailgun answered {status}: {}",
            String::from_utf8_lossy(response.body()).chars().take(300).collect::<String>()
        );
        Err(if matches!(status, StatusCode::BAD_REQUEST | StatusCode::PAYLOAD_TOO_LARGE) {
            MailError::Permanent(text)
        } else {
            MailError::Transient(text)
        })
    }
}

/// Whether `signature` is Mailgun's webhook signature for `timestamp` and
/// `token`.
///
/// The signature is the hex HMAC-SHA256 of `timestamp` then `token`, keyed
/// with the webhook signing key. The comparison takes the same time for any
/// wrong signature of the right length.
#[must_use]
pub fn verify_mailgun_signature(key: &str, timestamp: &str, token: &str, signature: &str) -> bool {
    let Ok(expected) = hex::decode(signature) else {
        return false;
    };
    let Ok(mut mac) = hmac::Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes()) else {
        return false;
    };
    mac.update(timestamp.as_bytes());
    mac.update(token.as_bytes());
    mac.verify_slice(&expected).is_ok()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Core can't use bizkit-testkit's `FakeHttp`: testkit depends on core.
    #[derive(Debug)]
    struct Canned {
        status: StatusCode,
        body: &'static str,
        seen: Mutex<Vec<http::Request<Bytes>>>,
    }

    #[async_trait]
    impl HttpClient for Canned {
        async fn send(&self, req: http::Request<Bytes>) -> anyhow::Result<http::Response<Bytes>> {
            self.seen.lock().unwrap().push(req);
            Ok(http::Response::builder().status(self.status).body(Bytes::from(self.body))?)
        }
    }

    fn mailer(
        region: MailgunRegion,
        status: StatusCode,
        body: &'static str,
    ) -> (MailgunMailer, Arc<Canned>) {
        let http = Arc::new(Canned { status, body, seen: Mutex::new(Vec::new()) });
        let config = MailgunConfig {
            domain: "mg.example.com".to_owned(),
            region,
            api_key_env: "UNUSED".to_owned(),
        };
        let mailer = MailgunMailer::with_key(
            Arc::<Canned>::clone(&http),
            &config,
            "Tallyho <news@mg.example.com>",
            "key-123",
        )
        .unwrap();
        (mailer, http)
    }

    fn mail() -> Mail {
        Mail {
            to: "ada@example.com".to_owned(),
            subject: "Hello".to_owned(),
            text: "Hi".to_owned(),
            html: Some("<p>Hi</p>".to_owned()),
            unsubscribe: Some(Url::parse("https://hub.example.com/u?t=abc").unwrap()),
            id: Some("0190-abc".to_owned()),
        }
    }

    #[tokio::test]
    async fn posts_the_form_with_basic_auth_and_returns_the_id() {
        let (mailer, http) = mailer(
            MailgunRegion::Us,
            StatusCode::OK,
            r#"{"id":"<2026.abc@mg.example.com>","message":"Queued."}"#,
        );
        let id = mailer.send(&mail()).await.unwrap();
        assert_eq!(id.as_deref(), Some("2026.abc@mg.example.com"), "id without brackets");

        let req = http.seen.lock().unwrap().remove(0);
        assert_eq!(req.uri(), "https://api.mailgun.net/v3/mg.example.com/messages", "endpoint");
        assert_eq!(
            req.headers()[http::header::AUTHORIZATION],
            "Basic YXBpOmtleS0xMjM=",
            "api:key-123"
        );
        let form: Vec<(String, String)> =
            url::form_urlencoded::parse(req.body()).into_owned().collect();
        insta::assert_debug_snapshot!(form, @r#"
        [
            (
                "from",
                "Tallyho <news@mg.example.com>",
            ),
            (
                "to",
                "ada@example.com",
            ),
            (
                "subject",
                "Hello",
            ),
            (
                "text",
                "Hi",
            ),
            (
                "html",
                "<p>Hi</p>",
            ),
            (
                "h:Message-Id",
                "<0190-abc@mg.example.com>",
            ),
            (
                "v:bizkit_id",
                "0190-abc",
            ),
            (
                "h:List-Unsubscribe",
                "<https://hub.example.com/u?t=abc>",
            ),
            (
                "h:List-Unsubscribe-Post",
                "List-Unsubscribe=One-Click",
            ),
        ]
        "#);
    }

    #[tokio::test]
    async fn eu_uses_the_eu_host() {
        let (mailer, http) = mailer(MailgunRegion::Eu, StatusCode::OK, r#"{"id":"<x@y>"}"#);
        mailer.send(&mail()).await.unwrap();
        assert_eq!(
            http.seen.lock().unwrap()[0].uri(),
            "https://api.eu.mailgun.net/v3/mg.example.com/messages",
            "endpoint"
        );
    }

    #[tokio::test]
    async fn mail_without_an_id_lets_mailgun_pick_one() {
        let (mailer, http) = mailer(MailgunRegion::Us, StatusCode::OK, r#"{"id":"<x@y>"}"#);
        mailer.send(&Mail { id: None, unsubscribe: None, ..mail() }).await.unwrap();
        let body = http.seen.lock().unwrap()[0].body().clone();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(!body.contains("Message-Id") && !body.contains("bizkit_id"), "{body}");
    }

    #[tokio::test]
    async fn statuses_map_to_error_classes() {
        for (status, permanent) in [
            (StatusCode::BAD_REQUEST, true),
            (StatusCode::PAYLOAD_TOO_LARGE, true),
            (StatusCode::UNAUTHORIZED, false),
            (StatusCode::FORBIDDEN, false),
            (StatusCode::NOT_FOUND, false),
            (StatusCode::TOO_MANY_REQUESTS, false),
            (StatusCode::INTERNAL_SERVER_ERROR, false),
        ] {
            let (mailer, _) = mailer(MailgunRegion::Us, status, r#"{"message":"no"}"#);
            let err = mailer.send(&mail()).await.unwrap_err();
            assert_eq!(err.is_permanent(), permanent, "{status}: {err}");
        }
    }

    #[test]
    fn verifies_signatures() {
        // printf '1700000000abc' | openssl dgst -sha256 -hmac key
        let computed = "bd2f1af294b8152534265a2dada145799d31c54cb9bab55bf4ea75be1770a0e3";
        assert!(verify_mailgun_signature("key", "1700000000", "abc", computed), "valid");
        assert!(!verify_mailgun_signature("other", "1700000000", "abc", computed), "wrong key");
        assert!(!verify_mailgun_signature("key", "1700000001", "abc", computed), "wrong timestamp");
        assert!(!verify_mailgun_signature("key", "1700000000", "abc", "zz"), "not hex");
        assert!(!verify_mailgun_signature("key", "1700000000", "abc", ""), "empty");
    }
}
