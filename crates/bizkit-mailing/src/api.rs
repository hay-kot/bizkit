//! The mailing module's API types.

use bizkit_core::ids::ProjectId;
use bizkit_core::time::UnixMillis;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

bizkit_core::id_type!(ListId, "The ID of a mailing list.");
bizkit_core::id_type!(ContactId, "The ID of a contact on a mailing list.");
bizkit_core::id_type!(BroadcastId, "The ID of a broadcast.");
bizkit_core::id_type!(DeliveryId, "The ID of one sent or queued mail.");
bizkit_core::id_type!(SequenceId, "The ID of an email sequence.");
bizkit_core::id_type!(
    StepId,
    "The ID of a sequence step. It stays the same when the step is edited."
);
bizkit_core::id_type!(EnrollmentId, "The ID of a contact's run through a sequence.");

/// A mailing list. Contacts and their consent belong to one list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MailingList {
    /// The list ID.
    pub id: ListId,
    /// The project it belongs to.
    pub project_id: ProjectId,
    /// The display name. Mail shows it in the header and footer.
    pub name: String,
    /// What subscribers get, for the team.
    pub description: Option<String>,
    /// New addresses from the API must confirm by mail before they get
    /// anything else. Addresses from forms always must.
    pub double_opt_in: bool,
    /// Text at the bottom of every mail, such as a postal address.
    pub footer: Option<String>,
    /// Forms whose submitters are added to the list.
    pub form_ids: Vec<String>,
    /// Contacts by status. Only on get.
    pub counts: Option<ContactCounts>,
    /// When it was created.
    pub created_at: UnixMillis,
    /// When its settings last changed.
    pub updated_at: UnixMillis,
}

/// Contacts of a list by status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct ContactCounts {
    /// Waiting to confirm.
    pub pending: u64,
    /// Getting mail.
    pub subscribed: u64,
    /// Opted out; they get no more mail from the list.
    pub unsubscribed: u64,
}

/// Lists of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MailingListList {
    /// Sorted by name.
    pub items: Vec<MailingList>,
}

/// Where a contact stands with a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContactStatus {
    /// Sent a confirm mail; gets nothing else until they confirm.
    Pending,
    /// Gets the list's mail.
    Subscribed,
    /// Opted out. Final: the address gets no more mail from this list.
    Unsubscribed,
}

impl ContactStatus {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Subscribed => "subscribed",
            Self::Unsubscribed => "unsubscribed",
        }
    }

    /// Parses the stored text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(Self::Pending),
            "subscribed" => Some(Self::Subscribed),
            "unsubscribed" => Some(Self::Unsubscribed),
            _ => None,
        }
    }
}

/// An address on a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Contact {
    /// The contact ID.
    pub id: ContactId,
    /// The list.
    pub list_id: ListId,
    /// The address, in lower case.
    pub email: String,
    /// A display name for `{{ name }}`.
    pub name: Option<String>,
    /// Labels for sending to part of a list.
    pub tags: Vec<String>,
    /// Where the contact stands.
    pub status: ContactStatus,
    /// How the address arrived: `api`, `admin`, `seed` or `form:<form id>`.
    pub source: String,
    /// When they confirmed, or were added without confirming.
    pub subscribed_at: Option<UnixMillis>,
    /// When they opted out.
    pub unsubscribed_at: Option<UnixMillis>,
    /// When the address arrived.
    pub created_at: UnixMillis,
    /// When the contact last changed.
    pub updated_at: UnixMillis,
}

/// A page of contacts, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContactPage {
    /// The contacts.
    pub items: Vec<Contact>,
    /// Pass as `before` for the next page; absent on the last one.
    pub next_before: Option<ContactId>,
}

/// Where a broadcast stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastStatus {
    /// Editable; not sent.
    Draft,
    /// Queuing mail for the recipients.
    Sending,
    /// Every recipient's mail is queued or sent.
    Sent,
}

impl BroadcastStatus {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Sending => "sending",
            Self::Sent => "sent",
        }
    }

    /// Parses the stored text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "draft" => Some(Self::Draft),
            "sending" => Some(Self::Sending),
            "sent" => Some(Self::Sent),
            _ => None,
        }
    }
}

/// One mail to a list's subscribers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Broadcast {
    /// The broadcast ID.
    pub id: BroadcastId,
    /// The list it goes to.
    pub list_id: ListId,
    /// The subject line.
    pub subject: String,
    /// The body in Markdown. `{{ name }}` and `{{ email }}` are filled in.
    pub body_md: String,
    /// Only subscribers with this tag, if set.
    pub tag: Option<String>,
    /// Where it stands.
    pub status: BroadcastStatus,
    /// Subscribers it was queued for.
    pub recipients: u64,
    /// When sending started.
    pub sent_at: Option<UnixMillis>,
    /// Its deliveries by status; only when getting one broadcast.
    pub deliveries: Option<DeliveryCounts>,
    /// When it was created.
    pub created_at: UnixMillis,
    /// When it last changed.
    pub updated_at: UnixMillis,
}

/// A broadcast's deliveries by status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct DeliveryCounts {
    /// Waiting to go out, or retrying.
    pub queued: u64,
    /// Accepted by the mail server.
    pub sent: u64,
    /// Every attempt failed. "Resend to failed" queues these again.
    pub failed: u64,
    /// Not sent, because the contact left the list.
    pub skipped: u64,
}

/// What "Resend to failed" did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ResendReport {
    /// Failed deliveries put back in the queue; 0 when none had failed.
    pub requeued: u64,
}

/// Broadcasts of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BroadcastList {
    /// Newest first.
    pub items: Vec<Broadcast>,
}

/// What a delivery was for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryKind {
    /// The double opt-in mail.
    Confirm,
    /// A broadcast.
    Broadcast,
    /// A sequence step.
    Step,
    /// A test send to a workspace member.
    Test,
}

impl DeliveryKind {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Broadcast => "broadcast",
            Self::Step => "step",
            Self::Test => "test",
        }
    }

    /// Parses the stored text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "confirm" => Some(Self::Confirm),
            "broadcast" => Some(Self::Broadcast),
            "step" => Some(Self::Step),
            "test" => Some(Self::Test),
            _ => None,
        }
    }
}

/// Where a delivery stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// Waiting to be sent, or between attempts.
    Queued,
    /// The mail server took it.
    Sent,
    /// Every attempt failed; `error` says why.
    Failed,
    /// Not sent: the contact left the list first, or a script said skip.
    Skipped,
}

impl DeliveryStatus {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Sent => "sent",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    /// Parses the stored text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "queued" => Some(Self::Queued),
            "sent" => Some(Self::Sent),
            "failed" => Some(Self::Failed),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// One mail the module sent or will send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MailDelivery {
    /// The delivery ID.
    pub id: DeliveryId,
    /// What it was for.
    pub kind: DeliveryKind,
    /// The contact, unless it was a test send or the contact was deleted.
    pub contact_id: Option<ContactId>,
    /// The broadcast, for broadcast and test sends.
    pub broadcast_id: Option<BroadcastId>,
    /// The recipient.
    pub to_email: String,
    /// The subject line.
    pub subject: String,
    /// Where it stands.
    pub status: DeliveryStatus,
    /// Why the last attempt failed, or why it was skipped.
    pub error: Option<String>,
    /// Attempts of the current send; a resend starts again from 1.
    pub attempts: u32,
    /// How often "Resend to failed" queued it again.
    pub resends: u32,
    /// When it was queued.
    pub created_at: UnixMillis,
    /// When the mail server took it.
    pub sent_at: Option<UnixMillis>,
}

/// A page of deliveries, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MailDeliveryPage {
    /// The deliveries.
    pub items: Vec<MailDelivery>,
    /// Pass as `before` for the next page; absent on the last one.
    pub next_before: Option<DeliveryId>,
}

/// A rendered mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Preview {
    /// The subject line, with merge fields filled in.
    pub subject: String,
    /// The HTML part.
    pub html: String,
    /// The plain-text part.
    pub text: String,
}

/// A form a list can take addresses from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MailingForm {
    /// The form's ID.
    pub id: String,
    /// The form's name.
    pub name: String,
}

/// The forms of a project, for linking them to a list.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MailingFormList {
    /// Sorted by name. Empty when the forms module isn't installed.
    pub items: Vec<MailingForm>,
}
