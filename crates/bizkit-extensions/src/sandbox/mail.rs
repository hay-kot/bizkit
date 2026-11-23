//! `ctx.mail.send`: queues mail from a sandbox thread through core's
//! `SendMail` job, under per-run and per-script caps.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::auth::is_plausible_email;
use bizkit_core::deps::Mail;
use bizkit_core::ids::WorkspaceId;
use bizkit_core::jobs::{Enqueue, SendMail};
use serde::Deserialize;

use crate::api::ScriptId;

/// The most mail one run can send.
pub const MAX_MAILS_PER_RUN: u32 = 3;
/// The most mail one script can send in [`MAIL_WINDOW`].
pub const MAX_MAILS_PER_HOUR: i64 = 50;
/// The window of [`MAX_MAILS_PER_HOUR`].
const MAIL_WINDOW: Duration = Duration::from_hours(1);
/// The longest subject, in characters.
pub const MAX_SUBJECT_CHARS: usize = 200;
/// The largest text or HTML body.
pub const MAX_MAIL_BODY_BYTES: usize = 64 << 10;

/// Sends a run's mail. Each run gets its own, so the call count is per run.
#[derive(Debug, Clone)]
pub struct MailBridge {
    platform: Platform,
    handle: tokio::runtime::Handle,
    workspace_id: WorkspaceId,
    script_id: ScriptId,
    calls: Arc<AtomicU32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Message {
    to: String,
    subject: String,
    text: String,
    html: Option<String>,
}

impl MailBridge {
    /// A bridge that sends mail for `script_id`.
    #[must_use]
    pub fn new(
        platform: Platform,
        handle: tokio::runtime::Handle,
        workspace_id: WorkspaceId,
        script_id: ScriptId,
    ) -> Self {
        Self { platform, handle, workspace_id, script_id, calls: Arc::new(AtomicU32::new(0)) }
    }

    /// Queues one message and answers with the error, if any, for the
    /// prelude to throw.
    pub(super) fn send(&self, message: &str) -> Option<String> {
        self.queue(message).err()
    }

    fn queue(&self, message: &str) -> Result<(), String> {
        let message: Message =
            serde_json::from_str(message).map_err(|err| format!("mail.send: {err}"))?;
        let mail = check(message)?;
        if self.calls.fetch_add(1, Ordering::SeqCst) >= MAX_MAILS_PER_RUN {
            return Err(format!("a run can send at most {MAX_MAILS_PER_RUN} mails"));
        }
        let (platform, workspace_id, script_id) =
            (self.platform.clone(), self.workspace_id, self.script_id);
        let queued = self
            .handle
            .block_on(async move { enqueue(&platform, workspace_id, script_id, mail).await });
        match queued {
            Ok(true) => Ok(()),
            Ok(false) => {
                Err(format!("this script already sent {MAX_MAILS_PER_HOUR} mails in the last hour"))
            }
            Err(err) => {
                tracing::warn!(%script_id, "queueing script mail: {err:#}");
                Err("queueing the mail failed".to_owned())
            }
        }
    }
}

fn check(message: Message) -> Result<Mail, String> {
    let Message { to, subject, text, html } = message;
    let to = to.trim().to_owned();
    if !is_plausible_email(&to) {
        return Err(format!("mail.send: {to:?} is not an email address"));
    }
    if subject.chars().count() > MAX_SUBJECT_CHARS {
        return Err(format!("mail.send: the subject is over {MAX_SUBJECT_CHARS} characters"));
    }
    if subject.chars().any(char::is_control) {
        return Err("mail.send: the subject can't hold line breaks or control characters".into());
    }
    if subject.trim().is_empty() {
        return Err("mail.send: the subject is empty".to_owned());
    }
    for (name, body) in [("text", Some(&text)), ("html", html.as_ref())] {
        if body.is_some_and(|body| body.len() > MAX_MAIL_BODY_BYTES) {
            return Err(format!("mail.send: {name} is over {} KiB", MAX_MAIL_BODY_BYTES >> 10));
        }
    }
    Ok(Mail { to, subject, text, html })
}

/// Records the send and queues the mail in one write, or answers false
/// when the script is at its hourly cap.
async fn enqueue(
    platform: &Platform,
    workspace_id: WorkspaceId,
    script_id: ScriptId,
    mail: Mail,
) -> anyhow::Result<bool> {
    let now = platform.now();
    let since = now.minus(MAIL_WINDOW);
    let mut tx = platform.db().write().await.context("starting the mail transaction")?;
    let sent = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM extension_mail_sends
         WHERE workspace_id = ? AND script_id = ? AND sent_at > ?",
        workspace_id,
        script_id,
        since,
    )
    .fetch_one(&mut *tx)
    .await
    .context("counting recent mail")?;
    if sent >= MAX_MAILS_PER_HOUR {
        return Ok(false);
    }
    sqlx::query!(
        "INSERT INTO extension_mail_sends (workspace_id, script_id, sent_at) VALUES (?, ?, ?)",
        workspace_id,
        script_id,
        now,
    )
    .execute(&mut *tx)
    .await
    .context("recording the mail")?;
    platform
        .bus()
        .enqueue(&mut tx, &SendMail(mail), Enqueue::default())
        .await
        .context("queueing the mail")?;
    tx.commit().await.context("committing the mail")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(to: &str, subject: &str) -> Message {
        Message {
            to: to.to_owned(),
            subject: subject.to_owned(),
            text: "hi".to_owned(),
            html: None,
        }
    }

    #[test]
    fn check_refuses_bad_messages() {
        assert!(check(message(" a@example.com ", "Hello")).is_ok());
        for (to, subject, want) in [
            ("nope", "Hello", "not an email"),
            ("a@example.com", "", "empty"),
            ("a@example.com", "a\r\nBcc: x@example.com", "line breaks"),
            ("a@example.com", &"s".repeat(201), "over 200"),
        ] {
            let err = check(message(to, subject)).unwrap_err();
            assert!(err.contains(want), "{to} {subject}: {err}");
        }
        let mut big = message("a@example.com", "Hello");
        big.html = Some("x".repeat(MAX_MAIL_BODY_BYTES + 1));
        assert!(check(big).unwrap_err().contains("html is over"));
    }
}
