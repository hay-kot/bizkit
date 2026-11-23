//! The analytics module's API types.

use bizkit_core::ids::ProjectId;
use bizkit_core::time::UnixMillis;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

bizkit_core::id_type!(GoalId, "The ID of an analytics goal.");
bizkit_core::id_type!(FunnelId, "The ID of an analytics funnel.");
bizkit_core::id_type!(PersonId, "The ID of an identified person in analytics.");

/// What the snippet may store in a visitor's browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentityMode {
    /// Nothing. Visitors are a daily hash, and `identify` is ignored.
    #[default]
    Cookieless,
    /// An anonymous ID, only once the site calls `identify`.
    AfterIdentify,
    /// An anonymous ID from the first pageview. For sites that collect consent.
    Always,
}

impl IdentityMode {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cookieless => "cookieless",
            Self::AfterIdentify => "after_identify",
            Self::Always => "always",
        }
    }

    /// Reads the stored text; unknown text is the safest mode.
    #[must_use]
    pub fn from_stored(text: &str) -> Self {
        match text {
            "after_identify" => Self::AfterIdentify,
            "always" => Self::Always,
            _ => Self::Cookieless,
        }
    }
}

/// A project's analytics settings and snippet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Site {
    /// The project it belongs to.
    pub project_id: ProjectId,
    /// The public key in the snippet tag.
    pub key: String,
    /// The `<script>` tag to add to every page.
    pub snippet: String,
    /// Hostnames the snippet may send from, exact or `*.example.com`. Empty
    /// means any host.
    pub hostnames: Vec<String>,
    /// What the snippet may store in a visitor's browser.
    pub identity_mode: IdentityMode,
    /// Whether any event has arrived yet.
    pub has_events: bool,
    /// When analytics was turned on.
    pub created_at: UnixMillis,
    /// When its settings last changed.
    pub updated_at: UnixMillis,
}

/// What a goal matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    /// A pageview of exactly this path.
    Page,
    /// A custom event with exactly this name.
    Event,
}

impl GoalKind {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Event => "event",
        }
    }

    /// Reads the stored text.
    #[must_use]
    pub fn from_stored(text: &str) -> Self {
        if text == "event" { Self::Event } else { Self::Page }
    }
}

/// A page or event that counts as a conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Goal {
    /// The goal ID.
    pub id: GoalId,
    /// The project it belongs to.
    pub project_id: ProjectId,
    /// The display name.
    pub name: String,
    /// What it matches.
    pub kind: GoalKind,
    /// The page path (`/signup/done`) or event name (`signup_completed`).
    #[serde(rename = "match")]
    pub matches: String,
    /// When it was created.
    pub created_at: UnixMillis,
}

/// A project's goals.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoalList {
    /// Sorted by name.
    pub items: Vec<Goal>,
}
