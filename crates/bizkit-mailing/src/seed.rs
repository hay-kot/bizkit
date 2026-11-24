//! A sample list for the dev stack's demo workspace.

use std::time::Duration;

use bizkit_core::DemoSeed;
use bizkit_core::auth::random_token;
use bizkit_core::db::{Db, WriteTx};
use bizkit_core::time::UnixMillis;

use crate::api::{
    BroadcastId, BroadcastStatus, ContactId, ContactStatus, DeliveryId, DeliveryKind,
    DeliveryStatus, ListId,
};
use crate::capture::FORM_KIND;
use crate::store::{self, BroadcastRow, ContactRow, DeliveryRow, ListRow};

const LIST: &str = "Newsletter";

/// Address, name, status and days since the address arrived.
const CONTACTS: [(&str, Option<&str>, ContactStatus, u64); 5] = [
    ("ada@bizkit.test", Some("Ada"), ContactStatus::Subscribed, 30),
    ("grace@bizkit.test", Some("Grace"), ContactStatus::Subscribed, 21),
    ("linus@bizkit.test", None, ContactStatus::Subscribed, 9),
    ("margaret@bizkit.test", Some("Margaret"), ContactStatus::Pending, 0),
    ("ken@bizkit.test", Some("Ken"), ContactStatus::Unsubscribed, 14),
];

const SENT_BODY: &str = "Hi {{ name }},\n\nTallyho now exports to **CSV** and keeps a full \
history of every count. [See what changed](https://example.com/changelog).\n\nThanks for \
reading,\nThe Tallyho team";

const DRAFT_BODY: &str = "Hi {{ name }},\n\nAnnual billing is here: two months free when you \
pay for a year.\n\n- Same features\n- One invoice\n- Cancel any time\n\n\
[Switch to annual](https://example.com/pricing)";

fn days_ago(now: UnixMillis, days: u64) -> UnixMillis {
    now.minus(Duration::from_hours(24 * days))
}

/// Seeds the "Newsletter" list in the `website` project, linked to the
/// demo Contact form, with five contacts, one sent broadcast with its send
/// log and one draft. A list that already exists by name is left alone.
/// Nothing is queued, so the seed sends no mail.
pub(crate) async fn seed(db: &Db, seed: &mut DemoSeed) -> anyhow::Result<()> {
    let Some(&project_id) = seed.project_ids.get("website") else { return Ok(()) };
    let existing = store::list_lists(db.read(), seed.workspace_id, Some(project_id)).await?;
    if existing.iter().any(|list| list.name == LIST) {
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
    tx.commit().await?;
    seed.entities.insert(("mailing.list".to_owned(), LIST.to_owned()), list.id.to_string());
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
    for (email, name, status, age) in CONTACTS {
        let arrived = days_ago(now, age);
        let contact = ContactRow {
            id: ContactId::new(),
            workspace_id: list.workspace_id,
            list_id: list.id,
            email: email.to_owned(),
            name: name.map(str::to_owned),
            tags: if age > 20 { r#"["early"]"# } else { "[]" }.to_owned(),
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
        tag: None,
        status: BroadcastStatus::Sent.as_str().to_owned(),
        recipients: i64::try_from(subscribed.len()).unwrap_or(0),
        sent_at: Some(sent_at),
        created_at: days_ago(now, 4),
        updated_at: sent_at,
    };
    store::insert_broadcast(&mut **tx, &sent).await?;
    for contact in subscribed {
        let delivery = DeliveryRow {
            id: DeliveryId::new(),
            workspace_id: list.workspace_id,
            list_id: list.id,
            contact_id: Some(contact.id),
            kind: DeliveryKind::Broadcast.as_str().to_owned(),
            broadcast_id: Some(sent.id),
            enrollment_id: None,
            step_id: None,
            to_email: contact.email.clone(),
            subject: sent.subject.clone(),
            status: DeliveryStatus::Sent.as_str().to_owned(),
            error: None,
            attempts: 1,
            resends: 0,
            created_at: sent_at,
            sent_at: Some(sent_at.plus(Duration::from_secs(2))),
        };
        store::insert_delivery(&mut **tx, &delivery).await?;
    }

    let draft = BroadcastRow {
        id: BroadcastId::new(),
        subject: "Annual billing is here".to_owned(),
        body_md: DRAFT_BODY.to_owned(),
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
