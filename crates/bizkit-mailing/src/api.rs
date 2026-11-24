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
    /// Sent, then reported as a permanent bounce by the provider.
    pub bounced: u64,
    /// Sent, then reported as spam by the recipient.
    pub complained: u64,
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
    /// The enrollment, for sequence steps.
    pub enrollment_id: Option<EnrollmentId>,
    /// The sequence step, unless it was deleted.
    pub step_id: Option<StepId>,
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
    /// The provider's message ID, or the `Message-ID` bizkit set.
    pub provider_id: Option<String>,
    /// What the provider reported after sending, if anything.
    pub event: Option<DeliveryEvent>,
    /// When the provider reported `event`.
    pub event_at: Option<UnixMillis>,
}

/// What the mail provider reported about a sent mail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryEvent {
    /// The recipient's server refused it for good.
    Bounced,
    /// The recipient marked it as spam.
    Complained,
}

impl DeliveryEvent {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bounced => "bounced",
            Self::Complained => "complained",
        }
    }

    /// Parses the stored text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "bounced" => Some(Self::Bounced),
            "complained" => Some(Self::Complained),
            _ => None,
        }
    }
}

/// Why an address is suppressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionReason {
    /// A mail to it bounced for good.
    Bounce,
    /// The recipient reported a mail as spam. It can't be removed.
    Complaint,
    /// A team member added it.
    Manual,
}

impl SuppressionReason {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bounce => "bounce",
            Self::Complaint => "complaint",
            Self::Manual => "manual",
        }
    }

    /// Parses the stored text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "bounce" => Some(Self::Bounce),
            "complaint" => Some(Self::Complaint),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// An address no list in the workspace mails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Suppression {
    /// The address, lower case.
    pub email: String,
    /// Why it is suppressed.
    pub reason: SuppressionReason,
    /// The delivery that bounced or drew the complaint.
    pub delivery_id: Option<DeliveryId>,
    /// A team member's note, for manual entries.
    pub note: Option<String>,
    /// When it was added.
    pub created_at: UnixMillis,
}

/// A page of suppressions, by address.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SuppressionPage {
    /// The suppressions.
    pub items: Vec<Suppression>,
    /// Pass as `after` for the next page; absent on the last one.
    pub next_after: Option<String>,
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

/// What enrolls contacts in a sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SequenceTrigger {
    /// Contacts who become subscribed to the list after the sequence exists.
    OnJoin,
    /// Only `mailing.enrollments.create`.
    Manual,
}

impl SequenceTrigger {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OnJoin => "on_join",
            Self::Manual => "manual",
        }
    }

    /// Parses the stored text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "on_join" => Some(Self::OnJoin),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// One mail of a sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SequenceStep {
    /// The step ID. Edits keep it, so a contact never gets a step twice.
    pub id: StepId,
    /// Its place in the sequence, from 0.
    pub position: u32,
    /// Hours to wait after enrolling (first step) or after the step before.
    pub delay_hours: u32,
    /// The subject line.
    pub subject: String,
    /// The body in Markdown. `{{ name }}` and `{{ email }}` are filled in.
    pub body_md: String,
}

/// Enrollments of a sequence by status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct EnrollmentCounts {
    /// Still getting steps.
    pub active: u64,
    /// Got every step.
    pub completed: u64,
    /// Taken out of the sequence by the team.
    pub exited: u64,
    /// Left the list.
    pub unsubscribed: u64,
}

/// Timed mails that each enrolled contact of a list gets in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Sequence {
    /// The sequence ID.
    pub id: SequenceId,
    /// The list whose contacts it mails.
    pub list_id: ListId,
    /// The name, for the team.
    pub name: String,
    /// What enrolls contacts.
    pub trigger: SequenceTrigger,
    /// Whether new contacts enroll. Running enrollments go on either way.
    pub enabled: bool,
    /// The steps in order.
    pub steps: Vec<SequenceStep>,
    /// Enrollments by status; only when getting one sequence.
    pub enrollments: Option<EnrollmentCounts>,
    /// When it was created.
    pub created_at: UnixMillis,
    /// When it last changed.
    pub updated_at: UnixMillis,
}

/// Sequences of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SequenceList {
    /// Sorted by name.
    pub items: Vec<Sequence>,
}

/// Where a contact's run through a sequence stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnrollmentStatus {
    /// Waiting for the next step.
    Active,
    /// Every step was sent or skipped.
    Completed,
    /// Taken out by the team; gets no more steps.
    Exited,
    /// The contact unsubscribed; gets no more steps.
    Unsubscribed,
}

impl EnrollmentStatus {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Exited => "exited",
            Self::Unsubscribed => "unsubscribed",
        }
    }

    /// Parses the stored text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "active" => Some(Self::Active),
            "completed" => Some(Self::Completed),
            "exited" => Some(Self::Exited),
            "unsubscribed" => Some(Self::Unsubscribed),
            _ => None,
        }
    }
}

/// One contact's run through a sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Enrollment {
    /// The enrollment ID.
    pub id: EnrollmentId,
    /// The sequence.
    pub sequence_id: SequenceId,
    /// The contact.
    pub contact_id: ContactId,
    /// The contact's address.
    pub email: String,
    /// Where it stands.
    pub status: EnrollmentStatus,
    /// Current steps of the sequence it got, sent or skipped.
    pub steps_done: u32,
    /// When the next step is due, while active.
    pub next_run_at: Option<UnixMillis>,
    /// When the contact enrolled.
    pub created_at: UnixMillis,
    /// When it last changed.
    pub updated_at: UnixMillis,
}

/// A page of enrollments, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EnrollmentPage {
    /// The enrollments.
    pub items: Vec<Enrollment>,
    /// Pass as `before` for the next page; absent on the last one.
    pub next_before: Option<EnrollmentId>,
}
