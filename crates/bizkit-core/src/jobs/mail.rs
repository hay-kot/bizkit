use serde::{Deserialize, Serialize};

use super::{Job, JobCtx, JobHandler};
use crate::deps::{Mail, MailError};

/// Sends one mail. Mail goes through a job so SMTP failures retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendMail(pub Mail);

impl Job for SendMail {
    const KIND: &'static str = "mail.send";
    const MAX_ATTEMPTS: u32 = 8;
}

/// Hands [`SendMail`] jobs to the mail router. A permanent error is
/// logged and dropped, since a retry would fail the same way; a transient
/// one fails the attempt so the worker retries it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SendMailHandler;

impl JobHandler<SendMail> for SendMailHandler {
    async fn handle(&self, cx: JobCtx, job: SendMail) -> anyhow::Result<()> {
        match cx.platform.mail().send(&cx.platform, &job.0).await {
            Ok(_) => Ok(()),
            Err(MailError::Permanent(err)) => {
                tracing::warn!(to = %job.0.to, subject = %job.0.subject, "mail not sent: {err}");
                Ok(())
            }
            Err(err @ MailError::Transient(_)) => Err(err.into()),
        }
    }
}
