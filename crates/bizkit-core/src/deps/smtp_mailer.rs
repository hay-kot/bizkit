use std::env;

use anyhow::Context as _;
use async_trait::async_trait;
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

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, mail: &Mail) -> anyhow::Result<()> {
        let to: Mailbox =
            mail.to.parse().with_context(|| format!("parsing recipient {:?}", mail.to))?;
        let builder = Message::builder().from(self.from.clone()).to(to).subject(&mail.subject);
        let message = match &mail.html {
            Some(html) => builder
                .multipart(MultiPart::alternative_plain_html(mail.text.clone(), html.clone())),
            None => builder.body(mail.text.clone()),
        }
        .context("building the message")?;
        self.transport.send(message).await.context("sending over SMTP")?;
        Ok(())
    }
}
