//! The one place a contact's status changes: subscribe, confirm and
//! unsubscribe.

use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::auth::random_token;
use bizkit_core::db::WriteTx;
use bizkit_core::time::UnixMillis;

use crate::api::{Attributes, ContactId, ContactStatus, DeliveryKind};
use crate::store::{self, ContactRow, ListRow};
use crate::{send, sequences, suppress};

/// Confirm mails a list sends per hour. Past it, new addresses stay pending
/// without mail, so a script posting addresses to a public form can't turn
/// the server into a mail cannon.
pub const CONFIRMS_PER_HOUR: i64 = 100;
/// How long a confirm link works after its mail.
pub const CONFIRM_TTL: Duration = Duration::from_hours(24 * 7);
/// A pending contact gets another confirm mail at most this often.
pub const RESEND_AFTER: Duration = Duration::from_hours(1);

/// How an address arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    /// An API key.
    Api,
    /// A signed-in team member.
    Admin,
    /// A form linked to the list, by form ID.
    Form(String),
}

impl Source {
    fn as_text(&self) -> String {
        match self {
            Self::Api => "api".to_owned(),
            Self::Admin => "admin".to_owned(),
            Self::Form(id) => format!("form:{id}"),
        }
    }

    /// Form captures always confirm: a public form proves nothing about who
    /// typed the address. Other sources follow the list.
    const fn confirms(&self, list: &ListRow) -> bool {
        matches!(self, Self::Form(_)) || list.double_opt_in
    }
}

/// An address to add to a list.
#[derive(Debug, Clone)]
pub(crate) struct NewContact {
    /// Lower case, checked as plausible.
    pub(crate) email: String,
    pub(crate) name: Option<String>,
    pub(crate) tags: Vec<String>,
    /// Kept only for a new contact; an address already on the list keeps
    /// its own.
    pub(crate) attributes: Attributes,
    pub(crate) source: Source,
}

/// Adds `input` to `list`, or updates the address already there:
///
/// - new: pending with a confirm mail, or subscribed when the source
///   doesn't need to confirm;
/// - pending: another confirm mail, at most once per [`RESEND_AFTER`];
/// - subscribed: a missing name filled in, no mail;
/// - unsubscribed: unchanged and no mail, from any source;
/// - erased from the workspace: `None`, nothing stored.
pub(crate) async fn subscribe(
    platform: &Platform,
    tx: &mut WriteTx,
    list: &ListRow,
    input: NewContact,
) -> anyhow::Result<Option<ContactRow>> {
    if suppress::erased(&mut **tx, list.workspace_id, &input.email)
        .await
        .context("checking for an erasure")?
    {
        return Ok(None);
    }
    let now = platform.now();
    let existing =
        store::find_contact_by_email(&mut **tx, list.workspace_id, list.id, &input.email)
            .await
            .context("looking up the address")?;
    let Some(mut contact) = existing else {
        let confirms = input.source.confirms(list);
        let mut contact = ContactRow {
            id: ContactId::new(),
            workspace_id: list.workspace_id,
            list_id: list.id,
            email: input.email,
            name: input.name,
            tags: serde_json::to_string(&input.tags).context("encoding tags")?,
            attributes: serde_json::to_string(&input.attributes).context("encoding attributes")?,
            status: if confirms { ContactStatus::Pending } else { ContactStatus::Subscribed }
                .as_str()
                .to_owned(),
            source: input.source.as_text(),
            confirm_token: confirms.then(random_token),
            confirm_sent_at: None,
            unsubscribe_token: random_token(),
            subscribed_at: (!confirms).then_some(now),
            unsubscribed_at: None,
            created_at: now,
            updated_at: now,
        };
        store::insert_contact(&mut **tx, &contact).await.context("inserting the contact")?;
        if !confirms {
            sequences::enroll_on_join(platform, tx, &contact).await?;
        }
        if confirms && send_confirm(platform, tx, list, &contact).await? {
            contact.confirm_sent_at = Some(now);
            store::update_contact(&mut **tx, &contact)
                .await
                .context("recording the confirm mail")?;
        }
        return Ok(Some(contact));
    };
    let mut changed = false;
    match ContactStatus::parse(&contact.status) {
        Some(ContactStatus::Unsubscribed) | None => return Ok(Some(contact)),
        Some(ContactStatus::Pending) => {
            let due = contact.confirm_sent_at.is_none_or(|at| at <= now.minus(RESEND_AFTER));
            if due && send_confirm(platform, tx, list, &contact).await? {
                contact.confirm_sent_at = Some(now);
                changed = true;
            }
        }
        Some(ContactStatus::Subscribed) => {}
    }
    if contact.name.is_none() && input.name.is_some() {
        contact.name = input.name;
        changed = true;
    }
    if changed {
        contact.updated_at = now;
        store::update_contact(&mut **tx, &contact).await.context("updating the contact")?;
    }
    Ok(Some(contact))
}

/// Queues a confirm mail unless the list is over [`CONFIRMS_PER_HOUR`].
/// True when it was queued.
async fn send_confirm(
    platform: &Platform,
    tx: &mut WriteTx,
    list: &ListRow,
    contact: &ContactRow,
) -> anyhow::Result<bool> {
    let hour_ago = platform.now().minus(Duration::from_hours(1));
    let recent = store::confirms_since(&mut **tx, list.workspace_id, list.id, hour_ago)
        .await
        .context("counting confirm mails")?;
    if recent >= CONFIRMS_PER_HOUR {
        tracing::warn!(list_id = %list.id, "the list sent {recent} confirm mails in the past hour; holding the next one");
        return Ok(false);
    }
    let subject = format!("Confirm your subscription to {}", list.name);
    let mut row = send::delivery(platform, list, DeliveryKind::Confirm, &contact.email, subject);
    row.contact_id = Some(contact.id);
    send::queue(platform, tx, &row).await
}

/// What a confirm or unsubscribe link did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinkOutcome {
    /// The change happened, on the named list.
    Done(String),
    /// It had happened before, on the named list.
    Already(String),
    /// The confirm link is older than [`CONFIRM_TTL`].
    Expired,
    /// No contact has the token, or it can't change any more.
    Invalid,
}

/// Subscribes the pending contact a confirm link names and enrolls it in the
/// list's `on_join` sequences.
pub(crate) async fn confirm(
    platform: &Platform,
    tx: &mut WriteTx,
    token: &str,
) -> anyhow::Result<LinkOutcome> {
    let now = platform.now();
    let Some(mut contact) =
        store::find_by_confirm_token(&mut **tx, token).await.context("looking up the token")?
    else {
        return Ok(LinkOutcome::Invalid);
    };
    let Some(list) = store::find_list(&mut **tx, contact.workspace_id, contact.list_id)
        .await
        .context("loading the list")?
    else {
        return Ok(LinkOutcome::Invalid);
    };
    match ContactStatus::parse(&contact.status) {
        Some(ContactStatus::Subscribed) => return Ok(LinkOutcome::Already(list.name)),
        Some(ContactStatus::Unsubscribed) | None => return Ok(LinkOutcome::Invalid),
        Some(ContactStatus::Pending) => {}
    }
    if contact.confirm_sent_at.is_none_or(|at| at <= now.minus(CONFIRM_TTL)) {
        return Ok(LinkOutcome::Expired);
    }
    ContactStatus::Subscribed.as_str().clone_into(&mut contact.status);
    contact.subscribed_at = Some(now);
    contact.updated_at = now;
    store::update_contact(&mut **tx, &contact).await.context("subscribing the contact")?;
    sequences::enroll_on_join(platform, tx, &contact).await?;
    Ok(LinkOutcome::Done(list.name))
}

/// Whether the contact an unsubscribe link names is still on the list, by
/// list name. Changes nothing.
pub(crate) async fn unsubscribe_target(
    exec: impl sqlx::SqliteExecutor<'_> + Copy,
    token: &str,
) -> anyhow::Result<LinkOutcome> {
    let Some(contact) =
        store::find_by_unsubscribe_token(exec, token).await.context("looking up the token")?
    else {
        return Ok(LinkOutcome::Invalid);
    };
    let Some(list) = store::find_list(exec, contact.workspace_id, contact.list_id)
        .await
        .context("loading the list")?
    else {
        return Ok(LinkOutcome::Invalid);
    };
    Ok(if contact.status == ContactStatus::Unsubscribed.as_str() {
        LinkOutcome::Already(list.name)
    } else {
        LinkOutcome::Done(list.name)
    })
}

/// Unsubscribes the contact an unsubscribe link names.
pub(crate) async fn unsubscribe_by_token(
    tx: &mut WriteTx,
    token: &str,
    now: UnixMillis,
) -> anyhow::Result<LinkOutcome> {
    let Some(mut contact) =
        store::find_by_unsubscribe_token(&mut **tx, token).await.context("looking up the token")?
    else {
        return Ok(LinkOutcome::Invalid);
    };
    let Some(list) = store::find_list(&mut **tx, contact.workspace_id, contact.list_id)
        .await
        .context("loading the list")?
    else {
        return Ok(LinkOutcome::Invalid);
    };
    if contact.status == ContactStatus::Unsubscribed.as_str() {
        return Ok(LinkOutcome::Already(list.name));
    }
    unsubscribe(tx, &mut contact, now).await?;
    Ok(LinkOutcome::Done(list.name))
}

/// Marks `contact` unsubscribed and ends its active sequence runs. Final:
/// nothing subscribes it again.
pub(crate) async fn unsubscribe(
    tx: &mut WriteTx,
    contact: &mut ContactRow,
    now: UnixMillis,
) -> anyhow::Result<()> {
    if contact.status == ContactStatus::Unsubscribed.as_str() {
        return Ok(());
    }
    ContactStatus::Unsubscribed.as_str().clone_into(&mut contact.status);
    contact.unsubscribed_at = Some(now);
    contact.updated_at = now;
    store::update_contact(&mut **tx, contact).await.context("unsubscribing the contact")?;
    store::unsubscribe_enrollments(&mut **tx, contact.workspace_id, contact.id, now)
        .await
        .context("ending the contact's sequences")
}
