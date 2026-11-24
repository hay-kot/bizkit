//! A sample list for the dev stack's demo workspace.

use std::time::Duration;

use anyhow::Context as _;

use bizkit_core::DemoSeed;
use bizkit_core::auth::random_token;
use bizkit_core::db::{Db, WriteTx};
use bizkit_core::time::UnixMillis;

use crate::api::{
    BroadcastId, BroadcastStatus, ContactId, ContactStatus, DeliveryEvent, DeliveryId,
    DeliveryKind, DeliveryStatus, EnrollmentId, EnrollmentStatus, ListId, SequenceId,
    SequenceTrigger, StepId, SuppressionReason,
};
use crate::capture::FORM_KIND;
use crate::privacy;
use crate::store::{
    self, BroadcastRow, ContactRow, DeliveryFilter, DeliveryRow, EnrollmentRow, ListRow,
    SequenceRow, StepRow,
};
use crate::suppress::{self, Suppress};

const LIST: &str = "Newsletter";

/// Address, name, status, days since the address arrived and attributes.
const CONTACTS: [(&str, Option<&str>, ContactStatus, u64, &str); 5] = [
    (
        "ada@bizkit.test",
        Some("Ada"),
        ContactStatus::Subscribed,
        30,
        r#"{"city":"London","plan":"pro","seats":12}"#,
    ),
    (
        "grace@bizkit.test",
        Some("Grace"),
        ContactStatus::Subscribed,
        21,
        r#"{"city":"Arlington","plan":"pro","seats":3}"#,
    ),
    (
        "linus@bizkit.test",
        None,
        ContactStatus::Subscribed,
        9,
        r#"{"city":"Helsinki","plan":"free"}"#,
    ),
    ("margaret@bizkit.test", Some("Margaret"), ContactStatus::Pending, 0, r#"{"plan":"free"}"#),
    ("ken@bizkit.test", Some("Ken"), ContactStatus::Unsubscribed, 14, r#"{"plan":"pro"}"#),
];

const SENT_BODY: &str = "Hi {{ name }},\n\nTallyho now exports to **CSV** and keeps a full \
history of every count. [See what changed](https://example.com/changelog).\n\nThanks for \
reading,\nThe Tallyho team";

const DRAFT_BODY: &str = "Hi {{ name }},\n\nAnnual billing is here for {{ attr.plan }} plans: two \
months free when you pay for a year.\n\n- Same features\n- One invoice\n- Cancel any time\n\n\
[Switch to annual](https://example.com/pricing)";

const SEQUENCE: &str = "Welcome series";
/// The second list of the project, and who is on it.
const UPDATES: &str = "Product updates";
const UPDATES_CONTACTS: [(&str, &str); 2] =
    [("ada@bizkit.test", "Ada"), ("linus@bizkit.test", "Linus")];
/// An address erased on request; adding it to a list is refused.
const ERASED: &str = "erased@bizkit.test";
/// The contact whose broadcast mail bounced.
const BOUNCED: &str = "grace@bizkit.test";
/// An address suppressed by hand; it is on no list.
const MANUAL: (&str, &str) = ("orders@bizkit.test", "Shared inbox; asked us to stop.");

/// Delay in hours, subject and body of each welcome step.
const STEPS: [(i64, &str, &str); 3] = [
    (
        0,
        "Welcome to Tallyho",
        "Hi {{ name }},\n\nThanks for joining. Over the next week we'll send two short \
         notes to help you count what matters.\n\nThe Tallyho team",
    ),
    (
        48,
        "Count anything in three steps",
        "Hi {{ name }},\n\n1. Make a tally\n2. Share the link\n3. Watch the numbers \
         climb\n\n[Open Tallyho](https://example.com/app)",
    ),
    (
        120,
        "Teams, exports and what's next",
        "Hi {{ name }},\n\nInvite your team, export to **CSV**, and tell us what to build \
         next by replying to this mail.",
    ),
];

/// Enrolled contacts: address, how many steps they got, and whether they
/// left before the end.
const ENROLLED: [(&str, usize, bool); 2] =
    [("ada@bizkit.test", 3, false), ("grace@bizkit.test", 2, true)];

fn days_ago(now: UnixMillis, days: u64) -> UnixMillis {
    now.minus(Duration::from_hours(24 * days))
}

/// Seeds the "Newsletter" list in the `website` project, linked to the
/// demo Contact form, with five contacts, one sent broadcast with its send
/// log, one draft and the welcome sequence. A list that already exists by
/// name only gets the sequence, if it lacks one by that name, and the
/// suppressions. Nothing is queued, so the seed sends no mail.
pub(crate) async fn seed(db: &Db, seed: &mut DemoSeed) -> anyhow::Result<()> {
    let Some(&project_id) = seed.project_ids.get("website") else { return Ok(()) };
    let existing = store::list_lists(db.read(), seed.workspace_id, Some(project_id)).await?;
    if let Some(list) = existing.into_iter().find(|list| list.name == LIST) {
        // Stacks seeded before sequences existed get the sequence alone.
        let mut tx = db.write().await?;
        seed_sequence(&mut tx, &list, seed.now).await?;
        seed_suppressions(&mut tx, &list, seed.now).await?;
        seed_privacy(&mut tx, &list, seed.now).await?;
        tx.commit().await?;
        seed.entities.insert(("mailing.list".to_owned(), LIST.to_owned()), list.id.to_string());
        return Ok(());
    }
    let now = seed.now;
    let list = ListRow {
        id: ListId::new(),
        workspace_id: seed.workspace_id,
        project_id,
        name: LIST.to_owned(),
        description: Some("Product news, about once a month.".to_owned()),
        double_opt_in: true,
        footer: Some("Tallyho, 1 Main Street, Springfield".to_owned()),
        created_at: days_ago(now, 30),
        updated_at: days_ago(now, 30),
    };
    let mut tx = db.write().await?;
    store::insert_list(&mut *tx, &list).await?;
    let form = seed.entities.get(&(FORM_KIND.to_owned(), "Contact".to_owned()));
    if let Some(form) = form {
        store::set_list_forms(&mut tx, list.workspace_id, list.id, std::slice::from_ref(form))
            .await?;
    }

    let subscribed = seed_contacts(&mut tx, &list, form, now).await?;
    seed_broadcasts(&mut tx, &list, &subscribed, now).await?;
    seed_sequence(&mut tx, &list, now).await?;
    seed_suppressions(&mut tx, &list, now).await?;
    seed_privacy(&mut tx, &list, now).await?;
    tx.commit().await?;
    seed.entities.insert(("mailing.list".to_owned(), LIST.to_owned()), list.id.to_string());
    Ok(())
}

/// A second list in the project, so a newsletter mail's manage page shows
/// two lists, and one erased address that lists refuse. Skips what is
/// already there.
async fn seed_privacy(
    tx: &mut WriteTx,
    newsletter: &ListRow,
    now: UnixMillis,
) -> anyhow::Result<()> {
    let existing =
        store::list_lists(&mut **tx, newsletter.workspace_id, Some(newsletter.project_id))
            .await
            .context("loading the project's lists")?;
    if !existing.iter().any(|list| list.name == UPDATES) {
        let list = ListRow {
            id: ListId::new(),
            name: UPDATES.to_owned(),
            description: Some("Release notes, when a version ships.".to_owned()),
            double_opt_in: false,
            created_at: days_ago(now, 20),
            updated_at: days_ago(now, 20),
            ..newsletter.clone()
        };
        store::insert_list(&mut **tx, &list).await.context("inserting the updates list")?;
        for (email, name) in UPDATES_CONTACTS {
            let arrived = days_ago(now, 20);
            let contact = ContactRow {
                id: ContactId::new(),
                workspace_id: list.workspace_id,
                list_id: list.id,
                email: email.to_owned(),
                name: Some(name.to_owned()),
                tags: "[]".to_owned(),
                attributes: "{}".to_owned(),
                status: ContactStatus::Subscribed.as_str().to_owned(),
                source: "seed".to_owned(),
                confirm_token: None,
                confirm_sent_at: None,
                unsubscribe_token: random_token(),
                subscribed_at: Some(arrived),
                unsubscribed_at: None,
                created_at: arrived,
                updated_at: arrived,
            };
            store::insert_contact(&mut **tx, &contact)
                .await
                .context("inserting an updates contact")?;
        }
    }
    privacy::erase(tx, newsletter.workspace_id, ERASED, days_ago(now, 1)).await?;
    Ok(())
}

/// Inserts the sample contacts and returns the subscribed ones.
async fn seed_contacts(
    tx: &mut WriteTx,
    list: &ListRow,
    form: Option<&String>,
    now: UnixMillis,
) -> anyhow::Result<Vec<ContactRow>> {
    let mut subscribed = Vec::new();
    for (email, name, status, age, attributes) in CONTACTS {
        let arrived = days_ago(now, age);
        let contact = ContactRow {
            id: ContactId::new(),
            workspace_id: list.workspace_id,
            list_id: list.id,
            email: email.to_owned(),
            name: name.map(str::to_owned),
            tags: if age > 20 { r#"["early"]"# } else { "[]" }.to_owned(),
            attributes: attributes.to_owned(),
            status: status.as_str().to_owned(),
            source: form.map_or_else(|| "seed".to_owned(), |form| format!("form:{form}")),
            confirm_token: Some(random_token()),
            confirm_sent_at: Some(arrived),
            unsubscribe_token: random_token(),
            subscribed_at: (status != ContactStatus::Pending)
                .then(|| arrived.plus(Duration::from_mins(5))),
            unsubscribed_at: (status == ContactStatus::Unsubscribed).then(|| days_ago(now, 2)),
            created_at: arrived,
            updated_at: arrived,
        };
        store::insert_contact(&mut **tx, &contact).await?;
        if status == ContactStatus::Subscribed {
            subscribed.push(contact);
        }
    }

    Ok(subscribed)
}

/// Inserts one sent broadcast with its send log, and one draft.
async fn seed_broadcasts(
    tx: &mut WriteTx,
    list: &ListRow,
    subscribed: &[ContactRow],
    now: UnixMillis,
) -> anyhow::Result<()> {
    let sent_at = days_ago(now, 3);
    let sent = BroadcastRow {
        id: BroadcastId::new(),
        workspace_id: list.workspace_id,
        list_id: list.id,
        subject: "What's new in Tallyho".to_owned(),
        body_md: SENT_BODY.to_owned(),
        segment: "{}".to_owned(),
        status: BroadcastStatus::Sent.as_str().to_owned(),
        recipients: i64::try_from(subscribed.len()).unwrap_or(0),
        sent_at: Some(sent_at),
        created_at: days_ago(now, 4),
        updated_at: sent_at,
        send_at: None,
        paused_at: None,
        pause_reason: None,
        failures_in_row: 0,
    };
    store::insert_broadcast(&mut **tx, &sent).await?;
    for contact in subscribed {
        let delivery = DeliveryRow {
            broadcast_id: Some(sent.id),
            ..sent_delivery(list, contact, DeliveryKind::Broadcast, &sent.subject, sent_at)
        };
        store::insert_delivery(&mut **tx, &delivery).await?;
    }

    let draft = BroadcastRow {
        id: BroadcastId::new(),
        subject: "Annual billing is here".to_owned(),
        body_md: DRAFT_BODY.to_owned(),
        segment: r#"{"tags":[],"attributes":[{"key":"plan","equals":"pro"}],"joined_after":null,"joined_before":null}"#.to_owned(),
        status: BroadcastStatus::Draft.as_str().to_owned(),
        recipients: 0,
        sent_at: None,
        created_at: days_ago(now, 1),
        updated_at: days_ago(now, 1),
        ..sent
    };
    store::insert_broadcast(&mut **tx, &draft).await?;
    Ok(())
}

/// Suppresses one address by hand, and marks the sent broadcast's mail to
/// [`BOUNCED`] as a bounce with its suppression, as the Mailgun webhook
/// would. Both steps skip what is already there.
async fn seed_suppressions(
    tx: &mut WriteTx,
    list: &ListRow,
    now: UnixMillis,
) -> anyhow::Result<()> {
    let manual = Suppress {
        workspace_id: list.workspace_id,
        email: MANUAL.0,
        reason: SuppressionReason::Manual,
        delivery_id: None,
        note: Some(MANUAL.1.to_owned()),
        now: days_ago(now, 6),
    };
    suppress::suppress(tx, manual).await?;

    let Some(contact) =
        store::find_contact_by_email(&mut **tx, list.workspace_id, list.id, BOUNCED)
            .await
            .context("finding the bounced contact")?
    else {
        return Ok(());
    };
    let filter = DeliveryFilter { contact_id: Some(contact.id), ..DeliveryFilter::default() };
    let deliveries = store::list_deliveries(&mut **tx, list.workspace_id, &filter, 50)
        .await
        .context("loading the contact's deliveries")?;
    let Some(delivery) = deliveries
        .into_iter()
        .find(|d| d.kind == DeliveryKind::Broadcast.as_str() && d.event.is_none())
    else {
        return Ok(());
    };
    let at = delivery.sent_at.unwrap_or(delivery.created_at).plus(Duration::from_mins(3));
    store::mark_delivery_event(
        &mut **tx,
        list.workspace_id,
        delivery.id,
        DeliveryEvent::Bounced.as_str(),
        at,
    )
    .await
    .context("marking the bounce")?;
    let bounce = Suppress {
        workspace_id: list.workspace_id,
        email: BOUNCED,
        reason: SuppressionReason::Bounce,
        delivery_id: Some(delivery.id),
        note: None,
        now: at,
    };
    suppress::suppress(tx, bounce).await?;
    Ok(())
}

/// A delivery the mail server took two seconds after `at`.
fn sent_delivery(
    list: &ListRow,
    contact: &ContactRow,
    kind: DeliveryKind,
    subject: &str,
    at: UnixMillis,
) -> DeliveryRow {
    DeliveryRow {
        id: DeliveryId::new(),
        workspace_id: list.workspace_id,
        list_id: list.id,
        contact_id: Some(contact.id),
        kind: kind.as_str().to_owned(),
        broadcast_id: None,
        enrollment_id: None,
        step_id: None,
        to_email: contact.email.clone(),
        subject: subject.to_owned(),
        status: DeliveryStatus::Sent.as_str().to_owned(),
        error: None,
        attempts: 1,
        resends: 0,
        created_at: at,
        sent_at: Some(at.plus(Duration::from_secs(2))),
        provider_id: None,
        event: None,
        event_at: None,
    }
}

/// Inserts the 3-step welcome sequence with one completed and one exited
/// enrollment and their step mails. Seeded enrollments are never active:
/// the seed can't queue the job that would advance them.
async fn seed_sequence(tx: &mut WriteTx, list: &ListRow, now: UnixMillis) -> anyhow::Result<()> {
    let existing = store::list_sequences(&mut **tx, list.workspace_id, None, Some(list.id)).await?;
    if existing.iter().any(|sequence| sequence.name == SEQUENCE) {
        return Ok(());
    }
    let created = days_ago(now, 30);
    let sequence = SequenceRow {
        id: SequenceId::new(),
        workspace_id: list.workspace_id,
        list_id: list.id,
        name: SEQUENCE.to_owned(),
        trigger: SequenceTrigger::OnJoin.as_str().to_owned(),
        enabled: true,
        created_at: created,
        updated_at: created,
    };
    store::insert_sequence(&mut **tx, &sequence).await?;
    let steps: Vec<StepRow> = STEPS
        .iter()
        .zip(0_i64..)
        .map(|((delay_hours, subject, body_md), position)| StepRow {
            id: StepId::new(),
            workspace_id: list.workspace_id,
            sequence_id: sequence.id,
            position,
            delay_hours: *delay_hours,
            subject: (*subject).to_owned(),
            body_md: (*body_md).to_owned(),
        })
        .collect();
    store::set_steps(tx, list.workspace_id, sequence.id, &steps).await?;

    for (email, reached, left) in ENROLLED {
        let Some(contact) =
            store::find_contact_by_email(&mut **tx, list.workspace_id, list.id, email).await?
        else {
            continue;
        };
        let mut at = contact.subscribed_at.unwrap_or(created);
        let enrollment = EnrollmentRow {
            id: EnrollmentId::new(),
            workspace_id: list.workspace_id,
            sequence_id: sequence.id,
            contact_id: contact.id,
            status: if left { EnrollmentStatus::Exited } else { EnrollmentStatus::Completed }
                .as_str()
                .to_owned(),
            next_run_at: None,
            created_at: at,
            updated_at: at,
        };
        if !store::insert_enrollment(&mut **tx, &enrollment).await? {
            continue;
        }
        for step in steps.iter().take(reached) {
            at = at.plus(Duration::from_hours(u64::try_from(step.delay_hours).unwrap_or(0)));
            let delivery = DeliveryRow {
                enrollment_id: Some(enrollment.id),
                step_id: Some(step.id),
                ..sent_delivery(list, &contact, DeliveryKind::Step, &step.subject, at)
            };
            store::insert_delivery(&mut **tx, &delivery).await?;
        }
        let ended = EnrollmentRow { updated_at: at, ..enrollment };
        store::update_enrollment(&mut **tx, &ended).await?;
    }
    Ok(())
}
