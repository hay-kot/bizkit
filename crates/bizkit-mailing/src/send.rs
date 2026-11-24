//! Sending: one `mailing.send` job per delivery row, and the
//! `mailing.broadcast` fan-out that queues a broadcast's deliveries in
//! batches. No job holds a write transaction across SMTP.

use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::db::WriteTx;
use bizkit_core::deps::{Mail, MailError};
use bizkit_core::ids::WorkspaceId;
use bizkit_core::jobs::{Enqueue, Job, JobCtx, JobHandler, Lane};
use bizkit_core::time::UnixMillis;
use serde::{Deserialize, Serialize};

use crate::api::{
    BroadcastId, BroadcastStatus, ContactStatus, DeliveryId, DeliveryKind, DeliveryStatus,
    EnrollmentStatus,
};
use crate::render::{self, Frame, Merge, Rendered};
use crate::store::{self, BroadcastRow, ContactRow, DeliveryRow, ListRow};
use crate::{manage, public, suppress};

/// The longest error text kept on a delivery.
const ERROR_CHARS: usize = 300;
/// Deliveries one fan-out transaction inserts.
pub const FAN_OUT_BATCH: i64 = 500;
/// Deliveries of a broadcast that fail in a row before it pauses. A broken
/// setup fails every mail, whatever the error class, so both count.
pub const BREAKER_FAILURES: i64 = 20;

/// The worker lane mailing jobs use, so a slow mail server never holds up
/// sign-in mail.
pub(crate) const MAILING: Lane = Lane { name: "mailing", concurrency: 2 };

/// Sends one delivery row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Send {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) delivery_id: DeliveryId,
}

impl Job for Send {
    const KIND: &'static str = "mailing.send";
    const MAX_ATTEMPTS: u32 = 8;
    const LANE: Lane = MAILING;
    const TIMEOUT: Duration = Duration::from_secs(60);
}

/// Queues the deliveries of a `sending` broadcast, one batch per run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FanOut {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) broadcast_id: BroadcastId,
}

impl Job for FanOut {
    const KIND: &'static str = "mailing.broadcast";
    const LANE: Lane = MAILING;
}

/// Starts a scheduled broadcast at its `send_at`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StartBroadcast {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) broadcast_id: BroadcastId,
    /// The `send_at` this job was queued for. A cancel, or a cancel and a
    /// new schedule, changes the row, so a stale job does nothing.
    pub(crate) due: UnixMillis,
}

impl Job for StartBroadcast {
    const KIND: &'static str = "mailing.broadcast.start";
    const LANE: Lane = MAILING;
}

/// Turns a draft into a sending broadcast and queues its fan-out.
pub(crate) async fn start(
    platform: &Platform,
    tx: &mut WriteTx,
    row: &mut BroadcastRow,
) -> anyhow::Result<()> {
    let now = platform.now();
    BroadcastStatus::Sending.as_str().clone_into(&mut row.status);
    row.sent_at = Some(now);
    row.updated_at = now;
    store::update_broadcast(&mut **tx, row).await.context("starting the broadcast")?;
    let job = FanOut { workspace_id: row.workspace_id, broadcast_id: row.id };
    platform.bus().enqueue(tx, &job, Enqueue::default()).await.context("queuing the fan-out")?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StartBroadcastHandler;

impl JobHandler<StartBroadcast> for StartBroadcastHandler {
    async fn handle(&self, cx: JobCtx, job: StartBroadcast) -> anyhow::Result<()> {
        let platform = &cx.platform;
        let mut tx = platform.db().write().await?;
        let Some(mut row) = store::find_broadcast(&mut *tx, job.workspace_id, job.broadcast_id)
            .await
            .context("loading the broadcast")?
        else {
            return Ok(());
        };
        if row.status != BroadcastStatus::Draft.as_str() || row.send_at != Some(job.due) {
            return Ok(());
        }
        start(platform, &mut tx, &mut row).await?;
        tx.commit().await
    }
}

/// Inserts `row` and enqueues its send, unless an equal delivery exists.
/// True when it was inserted.
pub(crate) async fn queue(
    platform: &Platform,
    tx: &mut WriteTx,
    row: &DeliveryRow,
) -> anyhow::Result<bool> {
    if !store::insert_delivery(&mut **tx, row).await.context("inserting a delivery")? {
        return Ok(false);
    }
    let job = Send { workspace_id: row.workspace_id, delivery_id: row.id };
    platform.bus().enqueue(tx, &job, Enqueue::default()).await?;
    Ok(true)
}

/// A new queued delivery.
pub(crate) fn delivery(
    platform: &Platform,
    list: &ListRow,
    kind: DeliveryKind,
    to_email: &str,
    subject: String,
) -> DeliveryRow {
    DeliveryRow {
        id: DeliveryId::new(),
        workspace_id: list.workspace_id,
        list_id: list.id,
        contact_id: None,
        kind: kind.as_str().to_owned(),
        broadcast_id: None,
        enrollment_id: None,
        step_id: None,
        to_email: to_email.to_owned(),
        subject,
        status: DeliveryStatus::Queued.as_str().to_owned(),
        error: None,
        attempts: 0,
        resends: 0,
        created_at: platform.now(),
        sent_at: None,
        provider_id: None,
        event: None,
        event_at: None,
    }
}

fn capped(text: &str) -> String {
    text.chars().take(ERROR_CHARS).collect()
}

/// The links a list mail to a contact carries.
#[derive(Debug, Clone, Default)]
struct Links {
    unsubscribe: Option<url::Url>,
    manage: Option<url::Url>,
}

impl Links {
    /// The contact's links, or none for mail to someone who is no contact.
    async fn of(platform: &Platform, contact: Option<&ContactRow>) -> anyhow::Result<Self> {
        let Some(contact) = contact else {
            return Ok(Self::default());
        };
        let key = manage::link_key(platform, contact.workspace_id).await?;
        Ok(Self {
            unsubscribe: Some(public::unsubscribe_url(platform, &contact.unsubscribe_token)?),
            manage: Some(manage::manage_url(platform, &key, contact)?),
        })
    }
}

/// The frame of a list's mail.
fn frame<'a>(list: &'a ListRow, links: &'a Links) -> Frame<'a> {
    Frame {
        list_name: &list.name,
        footer: list.footer.as_deref(),
        unsubscribe: links.unsubscribe.as_ref(),
        manage: links.manage.as_ref(),
    }
}

/// Renders a broadcast to one recipient.
pub(crate) async fn render_broadcast(
    platform: &Platform,
    list: &ListRow,
    broadcast: &BroadcastRow,
    contact: Option<&ContactRow>,
    to_email: &str,
) -> anyhow::Result<(Rendered, Option<url::Url>)> {
    let body = Body { subject: &broadcast.subject, md: &broadcast.body_md };
    render_to(platform, list, body, contact, to_email).await
}

/// A mail's source text.
#[derive(Debug, Clone, Copy)]
struct Body<'a> {
    subject: &'a str,
    md: &'a str,
}

/// Renders a list mail to one recipient, with their unsubscribe and manage
/// links when they are a contact.
async fn render_to(
    platform: &Platform,
    list: &ListRow,
    body: Body<'_>,
    contact: Option<&ContactRow>,
    to_email: &str,
) -> anyhow::Result<(Rendered, Option<url::Url>)> {
    let links = Links::of(platform, contact).await?;
    let merge = Merge { name: contact.and_then(|c| c.name.as_deref()), email: to_email };
    let mail = render::render(&frame(list, &links), body.subject, body.md, &merge);
    Ok((mail, links.unsubscribe))
}

/// A step delivery at send time: skipped once the contact left the
/// sequence or the step was deleted.
async fn prepare_step(
    platform: &Platform,
    list: &ListRow,
    delivery: &DeliveryRow,
    contact: &ContactRow,
) -> anyhow::Result<Prepared> {
    let db = platform.db().read();
    let Some(enrollment_id) = delivery.enrollment_id else {
        return Ok(Prepared::Skip("the sequence was deleted"));
    };
    let enrollment = store::find_enrollment(db, delivery.workspace_id, enrollment_id)
        .await
        .context("loading the enrollment")?;
    let left = enrollment.as_ref().is_none_or(|e| {
        e.status == EnrollmentStatus::Exited.as_str()
            || e.status == EnrollmentStatus::Unsubscribed.as_str()
    });
    if left {
        return Ok(Prepared::Skip("the contact left the sequence"));
    }
    let step = match delivery.step_id {
        Some(id) => {
            store::find_step(db, delivery.workspace_id, id).await.context("loading the step")?
        }
        None => None,
    };
    let Some(step) = step else {
        return Ok(Prepared::Skip("the step was deleted"));
    };
    let body = Body { subject: &step.subject, md: &step.body_md };
    let (mail, unsubscribe) =
        render_to(platform, list, body, Some(contact), &delivery.to_email).await?;
    Ok(Prepared::Mail(Box::new(to_mail(&delivery.to_email, mail, unsubscribe))))
}

/// What a delivery turns into at send time.
enum Prepared {
    Mail(Box<Mail>),
    Skip(&'static str),
}

async fn prepare(
    platform: &Platform,
    list: &ListRow,
    delivery: &DeliveryRow,
) -> anyhow::Result<Prepared> {
    let db = platform.db().read();
    let contact = match delivery.contact_id {
        Some(id) => store::find_contact(db, delivery.workspace_id, id)
            .await
            .context("loading the contact")?,
        None => None,
    };
    let status = contact.as_ref().and_then(|c| ContactStatus::parse(&c.status));
    let kind = DeliveryKind::parse(&delivery.kind)
        .with_context(|| format!("unknown delivery kind {:?}", delivery.kind))?;
    match kind {
        DeliveryKind::Confirm => {
            let Some(contact) = contact.filter(|_| status == Some(ContactStatus::Pending)) else {
                return Ok(Prepared::Skip("the contact is no longer waiting to confirm"));
            };
            let Some(token) = &contact.confirm_token else {
                return Ok(Prepared::Skip("the contact has no confirm link"));
            };
            let url = public::confirm_url(platform, token)?;
            let mail = render::confirm(&frame(list, &Links::default()), &url);
            Ok(Prepared::Mail(Box::new(to_mail(&contact.email, mail, None))))
        }
        DeliveryKind::Broadcast | DeliveryKind::Test => {
            if kind == DeliveryKind::Broadcast && status != Some(ContactStatus::Subscribed) {
                return Ok(Prepared::Skip("the contact is not subscribed"));
            }
            let Some(broadcast_id) = delivery.broadcast_id else {
                return Ok(Prepared::Skip("the broadcast was deleted"));
            };
            let Some(broadcast) = store::find_broadcast(db, delivery.workspace_id, broadcast_id)
                .await
                .context("loading the broadcast")?
            else {
                return Ok(Prepared::Skip("the broadcast was deleted"));
            };
            let contact = contact.filter(|_| kind == DeliveryKind::Broadcast);
            let (mail, unsubscribe) =
                render_broadcast(platform, list, &broadcast, contact.as_ref(), &delivery.to_email)
                    .await?;
            Ok(Prepared::Mail(Box::new(to_mail(&delivery.to_email, mail, unsubscribe))))
        }
        DeliveryKind::Step => {
            let Some(contact) = contact.filter(|_| status == Some(ContactStatus::Subscribed))
            else {
                return Ok(Prepared::Skip("the contact is not subscribed"));
            };
            prepare_step(platform, list, delivery, &contact).await
        }
    }
}

fn to_mail(to: &str, mail: Rendered, unsubscribe: Option<url::Url>) -> Mail {
    Mail {
        to: to.to_owned(),
        subject: mail.subject,
        text: mail.text,
        html: Some(mail.html),
        unsubscribe,
        id: None,
        workspace_id: None,
        reply_to: None,
    }
}

/// The suppression check every kind but `test` gets: a test send goes to
/// a team member who asked for it.
async fn suppressed(platform: &Platform, delivery: &DeliveryRow) -> anyhow::Result<Option<String>> {
    if delivery.kind == DeliveryKind::Test.as_str() {
        return Ok(None);
    }
    let reason = suppress::blocked(platform.db().read(), delivery.workspace_id, &delivery.to_email)
        .await
        .context("checking the suppression list")?;
    Ok(reason.map(|r| format!("suppressed: {}", r.as_str())))
}

async fn mark(platform: &Platform, delivery: &DeliveryRow) -> anyhow::Result<()> {
    mark_counting(platform, delivery, Count::Nothing).await
}

/// What a send attempt adds to its broadcast's breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Count {
    Nothing,
    Success,
    Failure,
}

/// Records the delivery and, for broadcast mail, updates the breaker in the
/// same transaction.
async fn mark_counting(
    platform: &Platform,
    delivery: &DeliveryRow,
    count: Count,
) -> anyhow::Result<()> {
    let mut tx = platform.db().write().await?;
    store::mark_delivery(&mut *tx, delivery).await.context("recording the delivery")?;
    let broadcast_id =
        delivery.broadcast_id.filter(|_| delivery.kind == DeliveryKind::Broadcast.as_str());
    if let Some(broadcast_id) = broadcast_id {
        match count {
            Count::Nothing => {}
            Count::Success => {
                store::broadcast_send_ok(&mut *tx, delivery.workspace_id, broadcast_id)
                    .await
                    .context("resetting the breaker")?;
            }
            Count::Failure => {
                let error = delivery.error.as_deref().unwrap_or("unknown error");
                let reason = capped(&format!(
                    "{BREAKER_FAILURES} deliveries failed in a row; the last: {error}"
                ));
                let paused = store::broadcast_send_failed(
                    &mut *tx,
                    delivery.workspace_id,
                    broadcast_id,
                    store::Pause { limit: BREAKER_FAILURES, reason: &reason, now: platform.now() },
                )
                .await
                .context("counting the failure")?;
                if paused {
                    tracing::warn!(broadcast = %broadcast_id, "paused a broadcast: {reason}");
                }
            }
        }
    }
    tx.commit().await
}

/// Whether the delivery belongs to a paused broadcast. Its job then parks
/// it instead of sending.
async fn paused(platform: &Platform, delivery: &DeliveryRow) -> anyhow::Result<bool> {
    let Some(broadcast_id) =
        delivery.broadcast_id.filter(|_| delivery.kind == DeliveryKind::Broadcast.as_str())
    else {
        return Ok(false);
    };
    let broadcast =
        store::find_broadcast(platform.db().read(), delivery.workspace_id, broadcast_id)
            .await
            .context("loading the broadcast")?;
    Ok(broadcast.is_some_and(|b| b.paused_at.is_some()))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SendHandler;

impl JobHandler<Send> for SendHandler {
    async fn handle(&self, cx: JobCtx, job: Send) -> anyhow::Result<()> {
        let platform = &cx.platform;
        let db = platform.db().read();
        let Some(mut delivery) = store::find_delivery(db, job.workspace_id, job.delivery_id)
            .await
            .context("loading the delivery")?
        else {
            return Ok(());
        };
        if delivery.status != DeliveryStatus::Queued.as_str() {
            return Ok(());
        }
        let Some(list) = store::find_list(db, delivery.workspace_id, delivery.list_id)
            .await
            .context("loading the list")?
        else {
            return Ok(());
        };
        if paused(platform, &delivery).await? {
            let mut tx = platform.db().write().await?;
            let parked =
                store::park_if_paused(&mut *tx, delivery.workspace_id, delivery.id, platform.now())
                    .await
                    .context("parking the delivery")?;
            tx.commit().await?;
            if parked {
                return Ok(());
            }
        }
        // An erase that commits after this check still lets this one mail
        // out: jobs hold no transaction across the send, by design.
        if let Some(reason) = suppressed(platform, &delivery).await? {
            DeliveryStatus::Skipped.as_str().clone_into(&mut delivery.status);
            delivery.error = Some(reason);
            return mark(platform, &delivery).await;
        }
        let mut mail = match prepare(platform, &list, &delivery).await? {
            Prepared::Mail(mail) => *mail,
            Prepared::Skip(reason) => {
                DeliveryStatus::Skipped.as_str().clone_into(&mut delivery.status);
                delivery.error = Some(reason.to_owned());
                return mark(platform, &delivery).await;
            }
        };
        mail.id = Some(delivery.id.to_string());
        mail.workspace_id = Some(delivery.workspace_id);
        delivery.attempts = i64::from(cx.attempt);
        let final_attempt = cx.attempt >= Send::MAX_ATTEMPTS;
        if final_attempt {
            // A timeout or panic on the last attempt must not leave the row
            // queued forever.
            DeliveryStatus::Failed.as_str().clone_into(&mut delivery.status);
            delivery.error = Some("the last attempt did not finish".to_owned());
            mark(platform, &delivery).await?;
        }
        // The breaker counts deliveries, not attempts: only a first attempt
        // that fails adds to it.
        let failure = if cx.attempt == 1 { Count::Failure } else { Count::Nothing };
        match platform.mail().send(platform, &mail).await {
            Ok(provider_id) => {
                DeliveryStatus::Sent.as_str().clone_into(&mut delivery.status);
                delivery.error = None;
                delivery.sent_at = Some(platform.now());
                delivery.provider_id = provider_id;
                mark_counting(platform, &delivery, Count::Success).await
            }
            // Retrying can't help, so the row fails now and the job ends.
            Err(MailError::Permanent(err)) => {
                DeliveryStatus::Failed.as_str().clone_into(&mut delivery.status);
                delivery.error = Some(capped(&err));
                mark_counting(platform, &delivery, failure).await
            }
            Err(err @ MailError::Transient(_)) => {
                let error = capped(&err.to_string());
                let status =
                    if final_attempt { DeliveryStatus::Failed } else { DeliveryStatus::Queued };
                status.as_str().clone_into(&mut delivery.status);
                delivery.error = Some(error.clone());
                mark_counting(platform, &delivery, failure).await?;
                // The job error lands in core's jobs table, which an erase
                // can't reach. Provider text can quote the recipient, so it
                // stays on the delivery row, and the job error names only
                // the delivery.
                anyhow::bail!("sending delivery {} failed; its row holds the error", delivery.id)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FanOutHandler;

impl JobHandler<FanOut> for FanOutHandler {
    async fn handle(&self, cx: JobCtx, job: FanOut) -> anyhow::Result<()> {
        let platform = &cx.platform;
        let mut tx = platform.db().write().await?;
        let Some(mut broadcast) =
            store::find_broadcast(&mut *tx, job.workspace_id, job.broadcast_id)
                .await
                .context("loading the broadcast")?
        else {
            return Ok(());
        };
        // A paused broadcast stops queuing; Resume queues this job again.
        if broadcast.status != BroadcastStatus::Sending.as_str() || broadcast.paused_at.is_some() {
            return Ok(());
        }
        let Some(list) = store::find_list(&mut *tx, broadcast.workspace_id, broadcast.list_id)
            .await
            .context("loading the list")?
        else {
            return Ok(());
        };
        let after = store::last_broadcast_contact(&mut *tx, broadcast.workspace_id, broadcast.id)
            .await
            .context("finding where the fan-out stopped")?;
        let batch = store::subscribed_after(&mut *tx, &broadcast, after, FAN_OUT_BATCH)
            .await
            .context("loading recipients")?;
        for (contact_id, email) in &batch {
            let mut row = delivery(
                platform,
                &list,
                DeliveryKind::Broadcast,
                email,
                broadcast.subject.clone(),
            );
            row.contact_id = Some(*contact_id);
            row.broadcast_id = Some(broadcast.id);
            queue(platform, &mut tx, &row).await?;
        }
        if i64::try_from(batch.len()).unwrap_or(i64::MAX) >= FAN_OUT_BATCH {
            platform.bus().enqueue(&mut tx, &job, Enqueue::default()).await?;
        } else {
            broadcast.recipients =
                store::broadcast_recipients(&mut *tx, broadcast.workspace_id, broadcast.id)
                    .await
                    .context("counting recipients")?;
            BroadcastStatus::Sent.as_str().clone_into(&mut broadcast.status);
            broadcast.updated_at = platform.now();
            store::update_broadcast(&mut *tx, &broadcast)
                .await
                .context("finishing the broadcast")?;
        }
        tx.commit().await
    }
}

#[cfg(test)]
mod tests {
    use bizkit_testkit::{Persona, TestApp};
    use http::StatusCode;
    use serde_json::{Value, json};

    use super::*;
    use crate::Mailing;

    #[tokio::test]
    async fn a_rerun_fan_out_queues_nobody_twice() {
        let app = TestApp::builder().module(Mailing).build().await;
        let team = app.seed(Persona::Team, "fan").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let ws = format!("/api/v1/workspaces/{}/mailing", team.workspaces["main"].id);
        let body =
            json!({"name": "News", "project_id": team.projects["site"].id, "double_opt_in": false});
        let list: Value =
            admin.post(&format!("{ws}/lists"), &body).await.expect(StatusCode::CREATED).json();
        let list_id = list["id"].as_str().unwrap();
        for i in 0..3 {
            let contact = json!({"email": format!("c{i}@example.com")});
            admin
                .post(&format!("{ws}/lists/{list_id}/contacts"), &contact)
                .await
                .expect(StatusCode::CREATED);
        }
        let draft = json!({"list_id": list_id, "subject": "Hi", "body_md": "Hi"});
        let broadcast: Value = admin
            .post(&format!("{ws}/broadcasts"), &draft)
            .await
            .expect(StatusCode::CREATED)
            .json();
        let broadcast_id: BroadcastId = broadcast["id"].as_str().unwrap().parse().unwrap();
        let path = format!("{ws}/broadcasts/{broadcast_id}/send");
        admin.post(&path, &json!({})).await.expect(StatusCode::OK);
        app.drain_jobs().await;
        assert_eq!(app.mail.take().len(), 3, "first run");

        // As if the worker died after the batch committed but before the
        // broadcast was marked sent.
        let platform = app.app().platform().clone();
        let workspace_id = team.workspaces["main"].id;
        let mut tx = platform.db().write().await.unwrap();
        let mut row =
            store::find_broadcast(&mut *tx, workspace_id, broadcast_id).await.unwrap().unwrap();
        BroadcastStatus::Sending.as_str().clone_into(&mut row.status);
        store::update_broadcast(&mut *tx, &row).await.unwrap();
        tx.commit().await.unwrap();
        let cx = JobCtx { platform: platform.clone(), job_id: 0, attempt: 1, request_id: None };
        FanOutHandler.handle(cx, FanOut { workspace_id, broadcast_id }).await.unwrap();
        app.drain_jobs().await;

        assert!(app.mail.take().is_empty(), "nobody mailed twice");
        let finished = store::find_broadcast(platform.db().read(), workspace_id, broadcast_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!((finished.status.as_str(), finished.recipients), ("sent", 3), "finished again");
    }
}
