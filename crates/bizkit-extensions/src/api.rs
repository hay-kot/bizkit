//! The extensions API types.

use bizkit_core::hooks::RunStatus;
use bizkit_core::ids::ProjectId;
use bizkit_core::time::UnixMillis;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Any JSON value, spelled as a type list so the document round-trips
/// through utoipa's typed model, which refuses a schema without `type`.
pub(crate) fn any_json() -> utoipa::openapi::Object {
    use utoipa::openapi::schema::{SchemaType, Type};
    utoipa::openapi::ObjectBuilder::new()
        .schema_type(SchemaType::from_iter([
            Type::Object,
            Type::Array,
            Type::String,
            Type::Number,
            Type::Boolean,
            Type::Null,
        ]))
        .build()
}

bizkit_core::id_type!(ScriptId, "The ID of an extension script.");
bizkit_core::id_type!(RunId, "The ID of a script run.");

/// When a hook point runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HookKindName {
    /// Inside the request, under a strict budget; fails open.
    Sync,
    /// As a retried job for each event.
    Async,
}

/// What a script on a hook point can use besides plain JavaScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// `ctx.log` and `console.log`.
    Log,
    /// `ctx.secrets`, the project's secrets.
    Secrets,
    /// `ctx.fetch` to public addresses.
    Fetch,
}

/// A hook point a compiled-in module declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Hook {
    /// The point ID, such as `forms.pre_validate`.
    pub id: String,
    /// What it is for and what it expects back.
    pub summary: String,
    /// When it runs.
    pub kind: HookKindName,
    /// The script's time budget, in milliseconds.
    pub budget_ms: u64,
    /// The event an async point runs on.
    pub event: Option<String>,
    /// What scripts on it can use.
    pub capabilities: Vec<Capability>,
    /// A sample input.
    #[schema(schema_with = any_json)]
    pub sample: serde_json::Value,
}

/// Every hook point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HookList {
    /// The points, in registration order.
    pub items: Vec<Hook>,
}

/// A script, without its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScriptSummary {
    /// The script.
    pub id: ScriptId,
    /// Its project.
    pub project_id: ProjectId,
    /// Its hook point.
    pub hook: String,
    /// Whether the hook point runs inside the request.
    pub sync: bool,
    /// The display name.
    pub name: String,
    /// Whether it runs.
    pub enabled: bool,
    /// False when no compiled-in module declares the hook point.
    pub available: bool,
    /// When it was created.
    pub created_at: UnixMillis,
    /// When it last changed.
    pub updated_at: UnixMillis,
}

/// A project's scripts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScriptList {
    /// The scripts, by name.
    pub items: Vec<ScriptSummary>,
}

/// A script with its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Script {
    /// The script.
    pub id: ScriptId,
    /// Its project.
    pub project_id: ProjectId,
    /// Its hook point.
    pub hook: String,
    /// Whether the hook point runs inside the request.
    pub sync: bool,
    /// The display name.
    pub name: String,
    /// The ES module source. Its default export runs.
    pub source: String,
    /// Whether it runs.
    pub enabled: bool,
    /// False when no compiled-in module declares the hook point.
    pub available: bool,
    /// When it was created.
    pub created_at: UnixMillis,
    /// When it last changed.
    pub updated_at: UnixMillis,
}

/// What started a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunCause {
    /// A sync hook call in a request.
    Sync,
    /// An event on an async hook point.
    Event,
    /// A test run from the editor or the API.
    Test,
}

impl RunCause {
    /// The stored name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Event => "event",
            Self::Test => "test",
        }
    }

    pub(crate) fn parse(text: &str) -> Self {
        match text {
            "sync" => Self::Sync,
            "event" => Self::Event,
            _ => Self::Test,
        }
    }
}

/// One script run. Logs, output and error have the project's secrets
/// replaced with `[secret:NAME]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Run {
    /// The run.
    pub id: RunId,
    /// The script.
    pub script_id: ScriptId,
    /// What started it.
    pub cause: RunCause,
    /// What it ended as.
    pub status: RunStatus,
    /// The job attempt, for event runs.
    pub attempt: Option<u32>,
    /// How long it took, in milliseconds.
    pub duration_ms: u64,
    /// Why it failed.
    pub error: Option<String>,
    /// What it logged.
    pub logs: Vec<String>,
    /// What it returned.
    #[schema(schema_with = any_json)]
    pub output: Option<serde_json::Value>,
    /// When it ran.
    pub created_at: UnixMillis,
}

/// A script's runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RunList {
    /// The runs, newest first.
    pub items: Vec<Run>,
}

/// A secret's name. Values are never returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Secret {
    /// The name, such as `DISCORD_WEBHOOK`.
    pub name: String,
    /// When it was last set.
    pub updated_at: UnixMillis,
}

/// A project's secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SecretList {
    /// The secrets, by name.
    pub items: Vec<Secret>,
}
