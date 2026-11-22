use serde::{Deserialize, Serialize};

use super::{Job, JobCtx, JobHandler};
use crate::deps::Mail;

/// Sends one mail. Mail goes through a job so SMTP failures retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendMail(pub Mail);

impl Job for SendMail {
    const KIND: &'static str = "mail.send";
    const MAX_ATTEMPTS: u32 = 8;
}

/// Hands [`SendMail`] jobs to the platform mailer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SendMailHandler;

impl JobHandler<SendMail> for SendMailHandler {
    async fn handle(&self, cx: JobCtx, job: SendMail) -> anyhow::Result<()> {
        cx.platform.deps().mailer.send(&job.0).await
    }
}
