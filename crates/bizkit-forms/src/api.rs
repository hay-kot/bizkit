//! The forms module's API types.

use std::collections::BTreeMap;

use bizkit_core::ids::ProjectId;
use bizkit_core::time::UnixMillis;
use serde::{Deserialize, Serialize};
use url::Url;
use utoipa::ToSchema;

bizkit_core::id_type!(FormId, "The ID of a form.");
bizkit_core::id_type!(SubmissionId, "The ID of a form submission.");

/// What a field holds, which decides how it is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// Any text.
    Text,
    /// An email address.
    Email,
    /// An http or https URL.
    Url,
    /// A finite number.
    Number,
    /// One of `options`, or several when `multiple`.
    Choice,
}

/// A field a form accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    /// The input's `name`: 1-64 of `a-z`, `A-Z`, `0-9`, `_` and `-`, not
    /// starting with `_`, unique in the form.
    pub name: String,
    /// A label for the UI and mail; the name when absent.
    #[serde(default)]
    pub label: Option<String>,
    /// How values are checked.
    pub kind: FieldKind,
    /// At least one non-blank value is needed.
    #[serde(default)]
    pub required: bool,
    /// The longest value, 1-10000 characters. Default 10000.
    #[serde(default)]
    pub max_length: Option<u32>,
    /// The allowed values of a `choice` field, 1-50 of them.
    #[serde(default)]
    pub options: Vec<String>,
    /// A `choice` field may take several values.
    #[serde(default)]
    pub multiple: bool,
}

/// How a form tells people from bots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BotCheck {
    /// Only the `_gotcha` honeypot field. Works with plain HTML forms.
    #[default]
    Honeypot,
    /// The honeypot, plus an ALTCHA proof-of-work challenge that also
    /// enforces the minimum fill time. Needs the widget script.
    Challenge,
}

impl BotCheck {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Honeypot => "honeypot",
            Self::Challenge => "challenge",
        }
    }
}

/// A form that collects submissions from a site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Form {
    /// The form ID, also its public address.
    pub id: FormId,
    /// The project it belongs to.
    pub project_id: ProjectId,
    /// The display name.
    pub name: String,
    /// Disabled forms answer 404 to submissions.
    pub enabled: bool,
    /// The accepted fields. Empty means every field is kept.
    pub fields: Vec<FieldDef>,
    /// The bot check.
    pub bot_check: BotCheck,
    /// With `challenge`: the least time in ms between loading the challenge and submitting.
    pub min_fill_ms: u32,
    /// Where browsers go after a plain form post. A thank-you page when absent.
    #[schema(value_type = Option<String>)]
    pub redirect_url: Option<Url>,
    /// Who gets mail for each new submission.
    pub notify_emails: Vec<String>,
    /// The URL sites post to.
    #[schema(value_type = String)]
    pub endpoint: Url,
    /// The URL the ALTCHA widget loads challenges from.
    #[schema(value_type = String)]
    pub challenge_url: Url,
    /// Submissions in the inbox.
    pub inbox_count: u64,
    /// When it was created.
    pub created_at: UnixMillis,
    /// When its settings last changed.
    pub updated_at: UnixMillis,
}

/// Forms of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FormList {
    /// Sorted by name.
    pub items: Vec<Form>,
}

/// Where a submission sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionStatus {
    /// Passed the bot check.
    #[default]
    Inbox,
    /// Failed the bot check, or marked as spam.
    Spam,
}

impl SubmissionStatus {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Spam => "spam",
        }
    }

    /// Parses the stored text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "inbox" => Some(Self::Inbox),
            "spam" => Some(Self::Spam),
            _ => None,
        }
    }
}

/// One field's submitted value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum FieldValue {
    /// A single value.
    One(String),
    /// Several values, such as checked boxes.
    Many(Vec<String>),
}

impl FieldValue {
    /// Every value.
    #[must_use]
    pub fn values(&self) -> &[String] {
        match self {
            Self::One(value) => std::slice::from_ref(value),
            Self::Many(values) => values,
        }
    }
}

/// What someone sent through a form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Submission {
    /// The submission ID.
    pub id: SubmissionId,
    /// The form.
    pub form_id: FormId,
    /// Inbox or spam.
    pub status: SubmissionStatus,
    /// Why the bot check flagged it, if it did.
    pub spam_reason: Option<String>,
    /// The values by field name.
    pub data: BTreeMap<String, FieldValue>,
    /// The sender's `User-Agent`, at most 512 characters.
    pub user_agent: Option<String>,
    /// The page that sent it (`Referer`), at most 512 characters.
    pub referer: Option<String>,
    /// When it arrived.
    pub created_at: UnixMillis,
}

/// A page of submissions, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubmissionList {
    /// The submissions.
    pub items: Vec<Submission>,
    /// Pass as `before` for the next page; absent on the last page.
    pub next_before: Option<SubmissionId>,
}

/// Submissions as CSV.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubmissionExport {
    /// A suggested file name.
    pub filename: String,
    /// The CSV text: a header row, then one row per submission, newest first.
    pub csv: String,
    /// True when more submissions exist than the export holds.
    pub truncated: bool,
}
