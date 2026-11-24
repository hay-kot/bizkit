//! Sending: one `mailing.send` job per delivery row, and the
//! `mailing.broadcast` fan-out that queues a broadcast's deliveries in
//! batches. No job holds a write transaction across SMTP.

use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::db::WriteTx;
use bizkit_core::deps::Mail;
use bizkit_core::ids::WorkspaceId;
use bizkit_core::jobs::{Enqueue, Job, JobCtx, JobHandler, Lane};
use serde::{Deserialize, Serialize};

use crate::api::{
    BroadcastId, BroadcastStatus, ContactStatus, DeliveryId, DeliveryKind, DeliveryStatus,
};
use crate::public;
use crate::render::{self, Frame, Merge, Rendered};
use crate::store::{self, BroadcastRow, ContactRow, DeliveryRow, ListRow};

/// The longest error text kept on a delivery.
const ERROR_CHARS: usize = 300;
/// Deliveries one fan-out transaction inserts.
pub const FAN_OUT_BATCH: i64 = 500;

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
    }
}

fn capped(text: &str) -> String {
    text.chars().take(ERROR_CHARS).collect()
}

/// The frame of a list's mail to `contact`.
fn frame<'a>(list: &'a ListRow, unsubscribe: Option<&'a url::Url>) -> Frame<'a> {
    Frame { list_name: &list.name, footer: list.footer.as_deref(), unsubscribe }
}

/// Renders a broadcast to one recipient.
pub(crate) fn render_broadcast(
    platform: &Platform,
    list: &ListRow,
    broadcast: &BroadcastRow,
    contact: Option<&ContactRow>,
    to_email: &str,
) -> anyhow::Result<(Rendered, Option<url::Url>)> {
    let unsubscribe =
        contact.map(|c| public::unsubscribe_url(platform, &c.unsubscribe_token)).transpose()?;
    let merge = Merge { name: contact.and_then(|c| c.name.as_deref()), email: to_email };
    let mail = render::render(
        &frame(list, unsubscribe.as_ref()),
        &broadcast.subject,
        &broadcast.body_md,
        &merge,
    );
    Ok((mail, unsubscribe))
}

/// What a delivery turns into at send time.
enum Prepared {
    Mail(Mail),
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
            let mail = render::confirm(&frame(list, None), &url);
            Ok(Prepared::Mail(to_mail(&contact.email, mail, None)))
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
                render_broadcast(platform, list, &broadcast, contact.as_ref(), &delivery.to_email)?;
            Ok(Prepared::Mail(to_mail(&delivery.to_email, mail, unsubscribe)))
        }
        DeliveryKind::Step => Ok(Prepared::Skip("the step was deleted")),
    }
}

fn to_mail(to: &str, mail: Rendered, unsubscribe: Option<url::Url>) -> Mail {
    Mail {
        to: to.to_owned(),
        subject: mail.subject,
        text: mail.text,
        html: Some(mail.html),
        unsubscribe,
    }
}

async fn mark(platform: &Platform, delivery: &DeliveryRow) -> anyhow::Result<()> {
    let mut tx = platform.db().write().await?;
    store::mark_delivery(&mut *tx, delivery).await.context("recording the delivery")?;
    tx.commit().await
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
        let mail = match prepare(platform, &list, &delivery).await? {
            Prepared::Mail(mail) => mail,
            Prepared::Skip(reason) => {
                DeliveryStatus::Skipped.as_str().clone_into(&mut delivery.status);
                delivery.error = Some(reason.to_owned());
                return mark(platform, &delivery).await;
            }
        };
        delivery.attempts = i64::from(cx.attempt);
        let final_attempt = cx.attempt >= Send::MAX_ATTEMPTS;
        if final_attempt {
            // A timeout or panic on the last attempt must not leave the row
            // queued forever.
            DeliveryStatus::Failed.as_str().clone_into(&mut delivery.status);
            delivery.error = Some("the last attempt did not finish".to_owned());
            mark(platform, &delivery).await?;
        }
        match platform.deps().mailer.send(&mail).await {
            Ok(()) => {
                DeliveryStatus::Sent.as_str().clone_into(&mut delivery.status);
                delivery.error = None;
                delivery.sent_at = Some(platform.now());
                mark(platform, &delivery).await
            }
            Err(err) => {
                let error = capped(&format!("{err:#}"));
                let status =
                    if final_attempt { DeliveryStatus::Failed } else { DeliveryStatus::Queued };
                status.as_str().clone_into(&mut delivery.status);
                delivery.error = Some(error.clone());
                mark(platform, &delivery).await?;
                anyhow::bail!("sending to {} failed: {error}", delivery.to_email)
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
        if broadcast.status != BroadcastStatus::Sending.as_str() {
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
