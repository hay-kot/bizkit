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
    /// `ctx.mail.send`, capped per run and per script.
    Mail,
}

/// The kind of entity scripts on a hook point can bind to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HookTargetKind {
    /// The kind ID, such as `forms.form`.
    pub id: String,
    /// The UI noun, such as `form`.
    pub noun: String,
    /// Its plural, such as `forms`.
    pub plural: String,
}

/// An entity a script runs for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScriptTarget {
    /// The entity ID, such as a form ID.
    pub id: String,
    /// Its display name; `None` when it no longer exists.
    pub label: Option<String>,
}

/// An entity scripts can target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TargetEntity {
    /// The entity ID, as `target_ids` takes it.
    pub id: String,
    /// Its display name.
    pub label: String,
}

/// A project's entities of one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TargetList {
    /// The entities, by label.
    pub items: Vec<TargetEntity>,
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
    /// The entity kind scripts on it can bind to; `None`: project-wide only.
    pub target: Option<HookTargetKind>,
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
    /// The entities it runs for, by ID. Empty: every entity of the point's
    /// kind, or the whole project when the point has no kind.
    pub targets: Vec<ScriptTarget>,
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
    /// The entities it runs for, by ID. Empty: every entity of the point's
    /// kind, or the whole project when the point has no kind.
    pub targets: Vec<ScriptTarget>,
    /// The display name.
    pub name: String,
    /// The ES module source. Its default export runs.
    pub source: String,
    /// The saved test cases.
    pub tests: Vec<TestCase>,
    /// Whether it runs.
    pub enabled: bool,
    /// False when no compiled-in module declares the hook point.
    pub available: bool,
    /// When it was created.
    pub created_at: UnixMillis,
    /// When it last changed.
    pub updated_at: UnixMillis,
}

/// A saved input to run a script on, and what it must return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TestCase {
    /// A name, unique in the script.
    pub name: String,
    /// The hook input.
    #[schema(schema_with = any_json)]
    pub input: serde_json::Value,
    /// The value the script must return, compared as JSON, with
    /// `undefined` as `null`. Absent or `null`: the run must only not fail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(schema_with = any_json)]
    pub expect: Option<serde_json::Value>,
}

/// How a test case went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestOutcome {
    /// The run succeeded and matched `expect`, if any.
    Pass,
    /// The run succeeded but returned something other than `expect`.
    Fail,
    /// The run failed: an error, a timeout or an invalid output.
    Error,
}

/// One test case's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TestResult {
    /// The case name.
    pub name: String,
    /// How it went.
    pub outcome: TestOutcome,
    /// The case's `expect`.
    #[schema(schema_with = any_json)]
    pub expect: Option<serde_json::Value>,
    /// The recorded run, with what the script returned and logged.
    pub run: Run,
}

/// A test run's results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TestReport {
    /// One result per case, in case order.
    pub items: Vec<TestResult>,
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
