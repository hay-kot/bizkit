use std::env;

use anyhow::Context as _;
use async_trait::async_trait;
use lettre::message::header::{HeaderName, HeaderValue};
use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport as _, Message, Tokio1Executor};
use url::Url;

use super::{Mail, MailError, Mailer};
use crate::config::MailConfig;

/// Sends mail over SMTP with rustls.
#[derive(Debug, Clone)]
pub struct SmtpMailer {
    from: Mailbox,
    reply_to: Option<Mailbox>,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpMailer {
    /// A mailer for `url`. The password comes from `config.smtp_password_env`
    /// when set, so it never sits in the config file.
    ///
    /// # Errors
    ///
    /// When `from` or the URL doesn't parse, or the password variable is unset.
    pub fn new(url: &Url, config: &MailConfig) -> anyhow::Result<Self> {
        let from: Mailbox =
            config.from.parse().with_context(|| format!("parsing mail.from {:?}", config.from))?;
        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::from_url(url.as_str())
            .with_context(|| format!("parsing mail.smtp_url {}", redact(url)))?;
        if let Some(var) = &config.smtp_password_env {
            let password =
                env::var(var).with_context(|| format!("reading the SMTP password from ${var}"))?;
            let user = percent_decode(url.username());
            builder = builder.credentials(Credentials::new(user, password));
        }
        Ok(Self { from, reply_to: None, transport: builder.build() })
    }

    /// A mailer over a transport the caller built, with a default
    /// `Reply-To`.
    pub(crate) const fn with_transport(
        from: Mailbox,
        reply_to: Option<Mailbox>,
        transport: AsyncSmtpTransport<Tokio1Executor>,
    ) -> Self {
        Self { from, reply_to, transport }
    }
}

fn percent_decode(text: &str) -> String {
    url::form_urlencoded::parse(format!("u={text}").as_bytes())
        .next()
        .map_or_else(|| text.to_owned(), |(_, v)| v.into_owned())
}

fn redact(url: &Url) -> String {
    let mut url = url.clone();
    // Ignored on purpose: these fail only for URLs that can't carry a password.
    let _ignored = url.set_password(None);
    url.to_string()
}

impl SmtpMailer {
    /// The `Message-ID` without angle brackets: `mail.id`, or a fresh UUID,
    /// at the `From` domain.
    fn message_id(&self, mail: &Mail) -> String {
        let local = mail.id.clone().unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        format!("{local}@{}", self.from.email.domain())
    }

    fn message(&self, mail: &Mail, message_id: &str) -> anyhow::Result<Message> {
        let to: Mailbox =
            mail.to.parse().with_context(|| format!("parsing recipient {:?}", mail.to))?;
        let mut builder = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(&mail.subject)
            .message_id(Some(format!("<{message_id}>")));
        let reply_to = match &mail.reply_to {
            Some(text) => Some(text.parse().with_context(|| format!("parsing reply-to {text:?}"))?),
            None => self.reply_to.clone(),
        };
        if let Some(reply_to) = reply_to {
            builder = builder.reply_to(reply_to);
        }
        if let Some(url) = &mail.unsubscribe {
            builder = builder
                .raw_header(HeaderValue::new(
                    HeaderName::new_from_ascii_str("List-Unsubscribe"),
                    format!("<{url}>"),
                ))
                .raw_header(HeaderValue::new(
                    HeaderName::new_from_ascii_str("List-Unsubscribe-Post"),
                    "List-Unsubscribe=One-Click".to_owned(),
                ));
        }
        match &mail.html {
            Some(html) => builder
                .multipart(MultiPart::alternative_plain_html(mail.text.clone(), html.clone())),
            None => builder.body(mail.text.clone()),
        }
        .context("building the message")
    }
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, mail: &Mail) -> Result<Option<String>, MailError> {
        let message_id = self.message_id(mail);
        // A message that doesn't build won't build on a retry either.
        let message = self
            .message(mail, &message_id)
            .map_err(|err| MailError::Permanent(format!("{err:#}")))?;
        self.transport.send(message).await.map_err(|err| classify(&err))?;
        Ok(Some(message_id))
    }
}

fn classify(err: &lettre::transport::smtp::Error) -> MailError {
    let text = format!("sending over SMTP: {err}");
    let code = err.status().map(u16::from);
    if err.is_permanent() && code.is_none_or(permanent_code) {
        MailError::Permanent(text)
    } else {
        MailError::Transient(text)
    }
}

/// 530 and 535 are 5xx but mean the server refused bizkit's credentials:
/// fixing the config makes the same mail go through.
const fn permanent_code(code: u16) -> bool {
    code >= 500 && code != 530 && code != 535
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mailer() -> SmtpMailer {
        let config =
            MailConfig { from: "bizkit <noreply@example.com>".to_owned(), ..MailConfig::default() };
        SmtpMailer::new(&Url::parse("smtp://localhost:1025").expect("url"), &config)
            .expect("mailer")
    }

    fn mail(unsubscribe: Option<&str>) -> Mail {
        Mail {
            unsubscribe: unsubscribe.map(|url| Url::parse(url).expect("url")),
            ..Mail::new("ada@example.com", "Hello", "Hi")
        }
    }

    fn formatted(mail: &Mail) -> String {
        let mailer = mailer();
        let id = mailer.message_id(mail);
        String::from_utf8(mailer.message(mail, &id).expect("message").formatted()).expect("utf-8")
    }

    #[test]
    fn list_mail_carries_one_click_unsubscribe_headers() {
        let text = formatted(&mail(Some("https://hub.example.com/u?t=abc")));
        assert!(text.contains("List-Unsubscribe: <https://hub.example.com/u?t=abc>"), "{text}");
        assert!(text.contains("List-Unsubscribe-Post: List-Unsubscribe=One-Click"), "{text}");
    }

    #[test]
    fn other_mail_has_no_unsubscribe_headers() {
        let text = formatted(&mail(None));
        assert!(!text.contains("List-Unsubscribe"), "{text}");
    }

    #[test]
    fn message_id_uses_the_mail_id_at_the_from_domain() {
        let with_id = Mail { id: Some("0190-abc".to_owned()), ..mail(None) };
        assert_eq!(mailer().message_id(&with_id), "0190-abc@example.com", "id");
        let text = formatted(&with_id);
        assert!(text.contains("Message-ID: <0190-abc@example.com>"), "{text}");
        assert!(mailer().message_id(&mail(None)).ends_with("@example.com"), "generated id");
    }

    #[test]
    fn reply_to_comes_from_the_mail_else_the_default() {
        let none = formatted(&mail(None));
        assert!(!none.contains("Reply-To"), "{none}");
        let with = Mail { reply_to: Some("ada@example.org".to_owned()), ..mail(None) };
        let given = formatted(&with);
        assert!(given.contains("Reply-To: ada@example.org"), "{given}");
        let defaulted = SmtpMailer::with_transport(
            "bizkit <noreply@example.com>".parse().expect("from"),
            Some("help@example.com".parse().expect("reply-to")),
            AsyncSmtpTransport::<Tokio1Executor>::unencrypted_localhost(),
        );
        let id = defaulted.message_id(&mail(None));
        let fallback =
            String::from_utf8(defaulted.message(&mail(None), &id).expect("message").formatted())
                .expect("utf-8");
        assert!(fallback.contains("Reply-To: help@example.com"), "{fallback}");
    }

    #[test]
    fn credential_refusals_are_not_permanent() {
        assert!(permanent_code(550), "mailbox unavailable");
        assert!(permanent_code(554), "rejected");
        assert!(!permanent_code(535), "auth failed");
        assert!(!permanent_code(530), "auth required");
        assert!(!permanent_code(451), "4xx");
    }

    #[tokio::test]
    async fn a_bad_recipient_is_permanent() {
        let mail = Mail { to: "not an address".to_owned(), ..mail(None) };
        let err = mailer().send(&mail).await.unwrap_err();
        assert!(err.is_permanent(), "{err}");
    }
}
