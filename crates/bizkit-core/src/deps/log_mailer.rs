use async_trait::async_trait;

use super::{Mail, Mailer};

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
    async fn send(&self, mail: &Mail) -> anyhow::Result<()> {
        tracing::info!(
            from = %self.from,
            to = %mail.to,
            subject = %mail.subject,
            "mail (no smtp_url set, so it is logged instead of sent):\n{}",
            mail.text
        );
        Ok(())
    }
}
