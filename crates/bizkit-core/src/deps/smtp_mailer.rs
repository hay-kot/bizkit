use std::env;

use anyhow::Context as _;
use async_trait::async_trait;
use lettre::message::header::{HeaderName, HeaderValue};
use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport as _, Message, Tokio1Executor};
use url::Url;

use super::{Mail, Mailer};
use crate::config::MailConfig;

/// Sends mail over SMTP with rustls.
#[derive(Debug, Clone)]
pub struct SmtpMailer {
    from: Mailbox,
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
        Ok(Self { from, transport: builder.build() })
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
    fn message(&self, mail: &Mail) -> anyhow::Result<Message> {
        let to: Mailbox =
            mail.to.parse().with_context(|| format!("parsing recipient {:?}", mail.to))?;
        let mut builder = Message::builder().from(self.from.clone()).to(to).subject(&mail.subject);
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
    async fn send(&self, mail: &Mail) -> anyhow::Result<()> {
        let message = self.message(mail)?;
        self.transport.send(message).await.context("sending over SMTP")?;
        Ok(())
    }
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
            to: "ada@example.com".to_owned(),
            subject: "Hello".to_owned(),
            text: "Hi".to_owned(),
            html: None,
            unsubscribe: unsubscribe.map(|url| Url::parse(url).expect("url")),
        }
    }

    #[test]
    fn list_mail_carries_one_click_unsubscribe_headers() {
        let message =
            mailer().message(&mail(Some("https://hub.example.com/u?t=abc"))).expect("message");
        let text = String::from_utf8(message.formatted()).expect("utf-8");
        assert!(text.contains("List-Unsubscribe: <https://hub.example.com/u?t=abc>"), "{text}");
        assert!(text.contains("List-Unsubscribe-Post: List-Unsubscribe=One-Click"), "{text}");
    }

    #[test]
    fn other_mail_has_no_unsubscribe_headers() {
        let message = mailer().message(&mail(None)).expect("message");
        let text = String::from_utf8(message.formatted()).expect("utf-8");
        assert!(!text.contains("List-Unsubscribe"), "{text}");
    }
}
