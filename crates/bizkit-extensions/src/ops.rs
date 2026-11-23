//! The extensions operations. Everything but the hooks list needs
//! `extensions:admin`: a script can read every module's hook data and send
//! the project's secrets anywhere, so writing one is admin power.

use anyhow::Context as _;
use bizkit_core::hooks::{HookKind, HookPoint};
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Level, Method, NoContent, OpCtx, Operation, Scope, Success};
use bizkit_core::validate::{FieldErrors, Validate, check_name};
use bizkit_core::{ApiError, ModuleContext, Platform};
use http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::api::{
    Capability, Hook, HookKindName, HookList, Run, RunCause, RunList, Script, ScriptId, ScriptList,
    ScriptSummary, Secret, SecretList,
};
use crate::runner::{ASYNC_BUDGET, Execution, MAX_SOURCE_BYTES, ScriptRunner};
use crate::store::{self, RunRow, ScriptRow};

const READ: Access = Access::Workspace(Scope::new("extensions", Level::Read));
const ADMIN: Access = Access::Workspace(Scope::new("extensions", Level::Admin));
const TAGS: &[&str] = &["extensions"];
const MAX_SECRET_BYTES: usize = 4 << 10;
const DEFAULT_RUNS: u32 = 50;
const MAX_RUNS: u32 = 100;

pub(crate) fn register(cx: &mut ModuleContext<'_>, runner: &ScriptRunner) -> anyhow::Result<()> {
    cx.scope_area("extensions", "Scripts on hook points, their secrets and runs")?;
    cx.op(HooksList)?;
    cx.op(ScriptsList)?;
    cx.op(ScriptsGet)?;
    cx.op(ScriptsCreate { runner: runner.clone() })?;
    cx.op(ScriptsUpdate { runner: runner.clone() })?;
    cx.op(ScriptsDelete)?;
    cx.op(ScriptsTest { runner: runner.clone() })?;
    cx.op(RunsList)?;
    cx.op(SecretsList)?;
    cx.op(SecretsPut)?;
    cx.op(SecretsDelete)?;
    Ok(())
}

// ---------------------------------------------------------------- conversions

fn hook_to_api(point: &HookPoint) -> Hook {
    let (kind, budget, event, capabilities) = match point.kind {
        HookKind::Sync { budget, .. } => (HookKindName::Sync, budget, None, vec![Capability::Log]),
        HookKind::Async { event } => (
            HookKindName::Async,
            ASYNC_BUDGET,
            Some(event.to_owned()),
            vec![Capability::Log, Capability::Secrets, Capability::Fetch],
        ),
    };
    Hook {
        id: point.id.to_owned(),
        summary: point.summary.to_owned(),
        kind,
        budget_ms: u64::try_from(budget.as_millis()).unwrap_or(u64::MAX),
        event,
        capabilities,
        sample: (point.sample)(),
    }
}

fn summary(platform: &Platform, row: ScriptRow) -> ScriptSummary {
    ScriptSummary {
        available: platform.hook_point(&row.hook).is_some(),
        id: row.id,
        project_id: row.project_id,
        hook: row.hook,
        sync: row.sync,
        name: row.name,
        enabled: row.enabled,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn script_to_api(platform: &Platform, row: ScriptRow) -> Script {
    Script {
        available: platform.hook_point(&row.hook).is_some(),
        id: row.id,
        project_id: row.project_id,
        hook: row.hook,
        sync: row.sync,
        name: row.name,
        source: row.source,
        enabled: row.enabled,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn run_to_api(row: RunRow) -> anyhow::Result<Run> {
    Ok(Run {
        id: row.id,
        script_id: row.script_id,
        cause: RunCause::parse(&row.cause),
        status: serde_json::from_value(serde_json::Value::String(row.status))
            .context("decoding a run status")?,
        attempt: row.attempt.and_then(|attempt| u32::try_from(attempt).ok()),
        duration_ms: u64::try_from(row.duration_ms).unwrap_or(0),
        error: row.error,
        logs: serde_json::from_str(&row.logs).context("decoding run logs")?,
        output: row
            .output
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .context("decoding run output")?,
        created_at: row.created_at,
    })
}

fn field_error(field: &str, message: impl Into<String>) -> ApiError {
    let mut errors = FieldErrors::default();
    errors.add(field, message);
    errors.into()
}

async fn script_in(
    cx: &OpCtx,
    workspace_id: WorkspaceId,
    script_id: ScriptId,
) -> Result<ScriptRow, ApiError> {
    store::find_script(cx.platform().db().read(), workspace_id, script_id)
        .await?
        .ok_or(ApiError::NotFound)
}

fn check_source(errors: &mut FieldErrors, source: &str) {
    if source.trim().is_empty() {
        errors.add("source", "must not be empty");
    } else if source.len() > MAX_SOURCE_BYTES {
        errors.add("source", format!("must be at most {} KiB", MAX_SOURCE_BYTES >> 10));
    }
}

fn is_secret_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_uppercase())
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

// ---------------------------------------------------------------- inputs

/// The workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InWorkspace {
    /// The workspace.
    #[expect(dead_code, reason = "a path param; the access check reads it from the path")]
    workspace_id: WorkspaceId,
}
impl Validate for InWorkspace {}

/// Which scripts to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListScripts {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// Only this project's scripts.
    #[serde(default)]
    project_id: Option<ProjectId>,
}
impl Validate for ListScripts {}

/// One script.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InScript {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The script.
    script_id: ScriptId,
}
impl Validate for InScript {}

/// A new script.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateScript {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project it belongs to.
    project_id: ProjectId,
    /// The hook point ID, such as `forms.pre_validate`. Fixed after create.
    hook: String,
    /// The display name.
    name: String,
    /// An ES module whose default export runs, at most 64 KiB.
    source: String,
    /// Whether it runs. Default true.
    #[serde(default)]
    enabled: Option<bool>,
}

impl Validate for CreateScript {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        check_source(&mut errors, &self.source);
        errors.into_result()
    }
}

/// Changes to a script. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateScript {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The script.
    script_id: ScriptId,
    /// The display name.
    #[serde(default)]
    name: Option<String>,
    /// The ES module source, at most 64 KiB.
    #[serde(default)]
    source: Option<String>,
    /// Whether it runs.
    #[serde(default)]
    enabled: Option<bool>,
}

impl Validate for UpdateScript {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(name) = &self.name {
            check_name(&mut errors, "name", name);
        }
        if let Some(source) = &self.source {
            check_source(&mut errors, source);
        }
        errors.into_result()
    }
}

/// A test run.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestScript {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The script.
    script_id: ScriptId,
    /// The input. Default the hook point's sample.
    #[serde(default)]
    #[schema(schema_with = crate::api::any_json)]
    input: Option<serde_json::Value>,
    /// Runs this source instead of the saved one, at most 64 KiB.
    #[serde(default)]
    source: Option<String>,
}

impl Validate for TestScript {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(source) = &self.source {
            check_source(&mut errors, source);
        }
        errors.into_result()
    }
}

/// Which runs to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListRuns {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The script.
    script_id: ScriptId,
    /// How many, 1-100. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}

impl Validate for ListRuns {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if self.limit.is_some_and(|limit| !(1..=MAX_RUNS).contains(&limit)) {
            errors.add("limit", "must be 1-100");
        }
        errors.into_result()
    }
}

/// A project.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InProject {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
}
impl Validate for InProject {}

/// A secret's new value.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PutSecret {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The name: an uppercase letter, then up to 63 uppercase letters, digits or `_`.
    name: String,
    /// The value, at most 4 KiB. Never returned.
    value: String,
}

impl Validate for PutSecret {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if !is_secret_name(&self.name) {
            errors.add("name", "must match ^[A-Z][A-Z0-9_]{0,63}$");
        }
        if self.value.is_empty() || self.value.len() > MAX_SECRET_BYTES {
            errors.add("value", "must be 1 byte to 4 KiB");
        }
        errors.into_result()
    }
}

/// One secret.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InSecret {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The secret name.
    name: String,
}
impl Validate for InSecret {}

// ---------------------------------------------------------------- hooks

struct HooksList;
impl Operation for HooksList {
    type Input = InWorkspace;
    type Output = HookList;
    const ID: &'static str = "extensions.hooks.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/extensions/hooks";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List the hook points scripts can attach to";
    const TAGS: &'static [&'static str] = TAGS;

    fn call(
        &self,
        cx: OpCtx,
        _input: InWorkspace,
    ) -> impl Future<Output = Result<HookList, ApiError>> + Send {
        let items = cx.platform().hook_points().iter().map(hook_to_api).collect();
        std::future::ready(Ok(HookList { items }))
    }
}

// ---------------------------------------------------------------- scripts

struct ScriptsList;
impl Operation for ScriptsList {
    type Input = ListScripts;
    type Output = ScriptList;
    const ID: &'static str = "extensions.scripts.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/extensions/scripts";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "List scripts, by name";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListScripts) -> Result<ScriptList, ApiError> {
        let rows =
            store::list_scripts(cx.platform().db().read(), input.workspace_id, input.project_id)
                .await?;
        Ok(ScriptList { items: rows.into_iter().map(|row| summary(cx.platform(), row)).collect() })
    }
}

struct ScriptsGet;
impl Operation for ScriptsGet {
    type Input = InScript;
    type Output = Script;
    const ID: &'static str = "extensions.scripts.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/extensions/scripts/{script_id}";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Get a script with its source";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InScript) -> Result<Script, ApiError> {
        let row = script_in(&cx, input.workspace_id, input.script_id).await?;
        Ok(script_to_api(cx.platform(), row))
    }
}

struct ScriptsCreate {
    runner: ScriptRunner,
}
impl Operation for ScriptsCreate {
    type Input = CreateScript;
    type Output = Script;
    const ID: &'static str = "extensions.scripts.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/extensions/scripts";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Attach a script to a hook point in a project";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: CreateScript) -> Result<Script, ApiError> {
        let Some(point) = cx.platform().hook_point(&input.hook) else {
            return Err(field_error("hook", "is not a hook point of this server"));
        };
        self.runner.check(&input.source).await.map_err(|err| field_error("source", err))?;
        let now = cx.now();
        let row = ScriptRow {
            id: ScriptId::new(),
            workspace_id: input.workspace_id,
            project_id: input.project_id,
            hook: point.id.to_owned(),
            sync: point.kind.is_sync(),
            name: input.name.trim().to_owned(),
            source: input.source,
            enabled: input.enabled.unwrap_or(true),
            created_at: now,
            updated_at: now,
        };
        let mut tx = cx.write().await?;
        if !bizkit_core::projects::exists(&mut *tx, input.workspace_id, input.project_id).await? {
            return Err(field_error("project_id", "is not a project of this workspace"));
        }
        if row.sync
            && store::has_sync_script(&mut *tx, row.workspace_id, row.project_id, &row.hook).await?
        {
            return Err(ApiError::Conflict(format!(
                "the project already has a script on {}; a sync hook point takes one",
                row.hook
            )));
        }
        store::insert_script(&mut *tx, &row).await?;
        tx.commit().await?;
        Ok(script_to_api(cx.platform(), row))
    }
}

struct ScriptsUpdate {
    runner: ScriptRunner,
}
impl Operation for ScriptsUpdate {
    type Input = UpdateScript;
    type Output = Script;
    const ID: &'static str = "extensions.scripts.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/extensions/scripts/{script_id}";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Change a script's name, source or enabled flag";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateScript) -> Result<Script, ApiError> {
        script_in(&cx, input.workspace_id, input.script_id).await?;
        if let Some(source) = &input.source {
            self.runner.check(source).await.map_err(|err| field_error("source", err))?;
        }
        // Read again under the write lock: the check can take a while, and a
        // concurrent PATCH or delete may have landed meanwhile.
        let mut tx = cx.write().await?;
        let mut row = store::find_script(&mut *tx, input.workspace_id, input.script_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if let Some(source) = input.source {
            row.source = source;
        }
        if let Some(name) = input.name {
            name.trim().clone_into(&mut row.name);
        }
        if let Some(enabled) = input.enabled {
            row.enabled = enabled;
        }
        row.updated_at = cx.now();
        store::update_script(&mut *tx, &row).await?;
        tx.commit().await?;
        Ok(script_to_api(cx.platform(), row))
    }
}

struct ScriptsDelete;
impl Operation for ScriptsDelete {
    type Input = InScript;
    type Output = NoContent;
    const ID: &'static str = "extensions.scripts.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/extensions/scripts/{script_id}";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Delete a script and its runs";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InScript) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let deleted = store::delete_script(&mut *tx, input.workspace_id, input.script_id).await?;
        tx.commit().await?;
        if deleted { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

struct ScriptsTest {
    runner: ScriptRunner,
}
impl Operation for ScriptsTest {
    type Input = TestScript;
    type Output = Run;
    const ID: &'static str = "extensions.scripts.test";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/extensions/scripts/{script_id}/test";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Run a script now with its hook point's capabilities, real fetch included, and record the run";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: TestScript) -> Result<Run, ApiError> {
        let row = script_in(&cx, input.workspace_id, input.script_id).await?;
        let Some(point) = cx.platform().hook_point(&row.hook) else {
            return Err(field_error("script_id", "its hook point isn't compiled into this server"));
        };
        let exec = Execution {
            script: &row,
            point,
            input: input.input.unwrap_or_else(point.sample),
            cause: RunCause::Test,
            attempt: None,
            source: input.source,
        };
        Ok(self.runner.execute(cx.platform(), exec).await)
    }
}

// ---------------------------------------------------------------- runs

struct RunsList;
impl Operation for RunsList {
    type Input = ListRuns;
    type Output = RunList;
    const ID: &'static str = "extensions.runs.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/extensions/runs";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "List a script's runs, newest first";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListRuns) -> Result<RunList, ApiError> {
        script_in(&cx, input.workspace_id, input.script_id).await?;
        let limit = i64::from(input.limit.unwrap_or(DEFAULT_RUNS));
        let rows =
            store::list_runs(cx.platform().db().read(), input.workspace_id, input.script_id, limit)
                .await?;
        let items = rows.into_iter().map(run_to_api).collect::<anyhow::Result<_>>()?;
        Ok(RunList { items })
    }
}

// ---------------------------------------------------------------- secrets

async fn project_in(
    cx: &OpCtx,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> Result<(), ApiError> {
    if bizkit_core::projects::exists(cx.platform().db().read(), workspace_id, project_id).await? {
        Ok(())
    } else {
        Err(ApiError::NotFound)
    }
}

struct SecretsList;
impl Operation for SecretsList {
    type Input = InProject;
    type Output = SecretList;
    const ID: &'static str = "extensions.secrets.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/extensions/projects/{project_id}/secrets";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "List a project's secret names";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InProject) -> Result<SecretList, ApiError> {
        project_in(&cx, input.workspace_id, input.project_id).await?;
        let rows =
            store::list_secrets(cx.platform().db().read(), input.workspace_id, input.project_id)
                .await?;
        let items =
            rows.into_iter().map(|(name, updated_at)| Secret { name, updated_at }).collect();
        Ok(SecretList { items })
    }
}

struct SecretsPut;
impl Operation for SecretsPut {
    type Input = PutSecret;
    type Output = Secret;
    const ID: &'static str = "extensions.secrets.put";
    const METHOD: Method = Method::Put;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/extensions/projects/{project_id}/secrets/{name}";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Set a project secret that async scripts read as ctx.secrets";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: PutSecret) -> Result<Secret, ApiError> {
        let now = cx.now();
        let mut tx = cx.write().await?;
        if !bizkit_core::projects::exists(&mut *tx, input.workspace_id, input.project_id).await? {
            return Err(ApiError::NotFound);
        }
        store::put_secret(
            &mut *tx,
            &store::SecretRow {
                workspace_id: input.workspace_id,
                project_id: input.project_id,
                name: &input.name,
                value: &input.value,
                updated_at: now,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(Secret { name: input.name, updated_at: now })
    }
}

struct SecretsDelete;
impl Operation for SecretsDelete {
    type Input = InSecret;
    type Output = NoContent;
    const ID: &'static str = "extensions.secrets.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/extensions/projects/{project_id}/secrets/{name}";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Delete a project secret";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InSecret) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let deleted =
            store::delete_secret(&mut *tx, input.workspace_id, input.project_id, &input.name)
                .await?;
        tx.commit().await?;
        if deleted { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_names() {
        for good in ["A", "DISCORD_WEBHOOK", "KEY2"] {
            assert!(is_secret_name(good), "{good}");
        }
        for bad in ["", "a", "1A", "A-B", "Ä", &"A".repeat(65)] {
            assert!(!is_secret_name(bad), "{bad}");
        }
    }
}
