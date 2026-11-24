use async_trait::async_trait;

use super::{Mail, MailError, Mailer};

/// Writes mail to the log instead of sending it, for installs without SMTP.
#[derive(Debug, Clone)]
pub struct LogMailer {
    from: String,
}

impl LogMailer {
    /// A mailer that logs as if sent from `from`.
    #[must_use]
    pub fn new(from: &str) -> Self {
        Self { from: from.to_owned() }
    }
}

#[async_trait]
impl Mailer for LogMailer {
    async fn send(&self, mail: &Mail) -> Result<Option<String>, MailError> {
        tracing::info!(
            from = %self.from,
            to = %mail.to,
            subject = %mail.subject,
            unsubscribe = mail.unsubscribe.as_ref().map(tracing::field::display),
            "mail (no smtp_url set, so it is logged instead of sent):\n{}",
            mail.text
        );
        Ok(None)
    }
}
