//! The analytics operations: site settings and goals.

use std::sync::Arc;

use anyhow::Context as _;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Level, Method, NoContent, OpCtx, Operation, Scope, Success};
use bizkit_core::validate::{FieldErrors, Validate, check_name};
use bizkit_core::{ApiError, ModuleContext, Platform};
use http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::api::{Goal, GoalId, GoalKind, GoalList, IdentityMode, Site};
use crate::ingest::Ingest;
use crate::names;
use crate::store::{self, GoalRow, SiteRow};

const READ: Access = Access::Workspace(Scope::new("analytics", Level::Read));
const ADMIN: Access = Access::Workspace(Scope::new("analytics", Level::Admin));
const TAGS: &[&str] = &["analytics"];
/// Allowed hostnames per site.
pub const MAX_HOSTNAMES: usize = 20;
const SITE_KEY_LEN: usize = 24;

pub(crate) fn register(cx: &mut ModuleContext<'_>, ingest: &Arc<Ingest>) -> anyhow::Result<()> {
    cx.scope_area("analytics", "Web analytics and product events")?;
    cx.op(SiteGet)?;
    cx.op(SiteCreate)?;
    cx.op(SiteUpdate(Arc::clone(ingest)))?;
    cx.op(GoalsList)?;
    cx.op(GoalsCreate(Arc::clone(ingest)))?;
    cx.op(GoalsDelete(Arc::clone(ingest)))?;
    cx.op(crate::server::EventsIngest(Arc::clone(ingest)))?;
    Ok(())
}

// ---------------------------------------------------------------- conversions

/// The `<script>` tag a site adds to its pages. The script is per site, so
/// it carries the key and the identity mode.
pub(crate) fn snippet_tag(platform: &Platform, site_key: &str) -> anyhow::Result<String> {
    let src = platform
        .base_url()
        .join(&format!("/m/analytics/public/s/{site_key}.js"))
        .context("building the snippet URL")?;
    Ok(format!(r#"<script defer src="{src}"></script>"#))
}

fn site_to_api(platform: &Platform, row: SiteRow) -> anyhow::Result<Site> {
    let hostnames: Vec<String> = serde_json::from_str(&row.hostnames)
        .with_context(|| format!("decoding the hostnames of project {}", row.project_id))?;
    Ok(Site {
        snippet: snippet_tag(platform, &row.site_key)?,
        project_id: row.project_id,
        key: row.site_key,
        hostnames,
        identity_mode: IdentityMode::from_stored(&row.identity_mode),
        has_events: row.last_event_day.is_some(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn goal_to_api(row: GoalRow) -> Goal {
    Goal {
        id: row.id,
        project_id: row.project_id,
        name: row.name,
        kind: GoalKind::from_stored(&row.kind),
        matches: row.matches,
        created_at: row.created_at,
    }
}

fn site_key() -> String {
    bizkit_core::auth::random_token().chars().take(SITE_KEY_LEN).collect()
}

fn normalize_hostnames(hostnames: &[String]) -> Vec<String> {
    let mut out: Vec<String> =
        hostnames.iter().map(|host| host.trim().to_ascii_lowercase()).collect();
    out.sort();
    out.dedup();
    out
}

fn check_hostnames(errors: &mut FieldErrors, hostnames: &[String]) {
    if hostnames.len() > MAX_HOSTNAMES {
        errors.add("hostnames", format!("must have at most {MAX_HOSTNAMES} entries"));
    }
    for host in hostnames {
        if !names::is_hostname_pattern(host.trim()) {
            errors.add("hostnames", format!("{host:?} must be a hostname, or *. and a hostname"));
        }
    }
}

async fn require_project(
    cx: &OpCtx,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> Result<(), ApiError> {
    if bizkit_core::projects::exists(cx.platform().db().read(), workspace_id, project_id).await? {
        return Ok(());
    }
    let mut errors = FieldErrors::default();
    errors.add("project_id", "is not a project of this workspace");
    Err(errors.into())
}

// ---------------------------------------------------------------- inputs

/// A project's analytics site.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InSite {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
}
impl Validate for InSite {}

/// Changes to a site. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateSite {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// Hostnames the snippet may send from, exact or `*.example.com`; empty
    /// allows any host.
    #[serde(default)]
    hostnames: Option<Vec<String>>,
    /// What the snippet may store in a visitor's browser.
    #[serde(default)]
    identity_mode: Option<IdentityMode>,
}

impl Validate for UpdateSite {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(hostnames) = &self.hostnames {
            check_hostnames(&mut errors, hostnames);
        }
        errors.into_result()
    }
}

/// Which goals to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListGoals {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
}
impl Validate for ListGoals {}

/// A new goal.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateGoal {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The display name.
    name: String,
    /// What it matches.
    kind: GoalKind,
    /// A page path starting with `/`, or an event name.
    #[serde(rename = "match")]
    matches: String,
}

impl Validate for CreateGoal {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        let matches = self.matches.trim();
        match self.kind {
            GoalKind::Page if !names::is_goal_path(matches) => {
                errors.add("match", "must be a path that starts with / (no * or query)");
            }
            GoalKind::Event if !names::is_event_name(matches) => {
                errors.add("match", names::EVENT_NAME_RULE);
            }
            GoalKind::Page | GoalKind::Event => {}
        }
        errors.into_result()
    }
}

/// A goal of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InGoal {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The goal.
    goal_id: GoalId,
}
impl Validate for InGoal {}

// ---------------------------------------------------------------- site

struct SiteGet;
impl Operation for SiteGet {
    type Input = InSite;
    type Output = Site;
    const ID: &'static str = "analytics.site.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/site";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a project's analytics settings and snippet";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InSite) -> Result<Site, ApiError> {
        let row = store::find_site(cx.platform().db().read(), input.workspace_id, input.project_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        Ok(site_to_api(cx.platform(), row)?)
    }
}

struct SiteCreate;
impl Operation for SiteCreate {
    type Input = InSite;
    type Output = Site;
    const ID: &'static str = "analytics.site.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/site";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Turn on analytics for a project";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: InSite) -> Result<Site, ApiError> {
        require_project(&cx, input.workspace_id, input.project_id).await?;
        let now = cx.now();
        let row = SiteRow {
            project_id: input.project_id,
            workspace_id: input.workspace_id,
            site_key: site_key(),
            hostnames: "[]".to_owned(),
            identity_mode: IdentityMode::default().as_str().to_owned(),
            last_event_day: None,
            rolled_through: None,
            created_at: now,
            updated_at: now,
        };
        let mut tx = cx.write().await?;
        if store::find_site(&mut *tx, input.workspace_id, input.project_id).await?.is_some() {
            return Err(ApiError::Conflict("analytics is already on for this project".to_owned()));
        }
        store::insert_site(&mut *tx, &row).await?;
        tx.commit().await?;
        Ok(site_to_api(cx.platform(), row)?)
    }
}

struct SiteUpdate(Arc<Ingest>);
impl Operation for SiteUpdate {
    type Input = UpdateSite;
    type Output = Site;
    const ID: &'static str = "analytics.site.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/site";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Change a project's analytics settings";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateSite) -> Result<Site, ApiError> {
        let mut tx = cx.write().await?;
        let mut row = store::find_site(&mut *tx, input.workspace_id, input.project_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if let Some(hostnames) = input.hostnames {
            row.hostnames = serde_json::to_string(&normalize_hostnames(&hostnames))
                .context("encoding hostnames")?;
        }
        if let Some(mode) = input.identity_mode {
            mode.as_str().clone_into(&mut row.identity_mode);
        }
        row.updated_at = cx.now();
        store::update_site_settings(&mut *tx, &row).await?;
        tx.commit().await?;
        self.0.sites.clear();
        Ok(site_to_api(cx.platform(), row)?)
    }
}

// ---------------------------------------------------------------- goals

struct GoalsList;
impl Operation for GoalsList {
    type Input = ListGoals;
    type Output = GoalList;
    const ID: &'static str = "analytics.goals.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/goals";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List a project's analytics goals";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListGoals) -> Result<GoalList, ApiError> {
        let rows =
            store::list_goals(cx.platform().db().read(), input.workspace_id, input.project_id)
                .await?;
        Ok(GoalList { items: rows.into_iter().map(goal_to_api).collect() })
    }
}

struct GoalsCreate(Arc<Ingest>);
impl Operation for GoalsCreate {
    type Input = CreateGoal;
    type Output = Goal;
    const ID: &'static str = "analytics.goals.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/goals";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Add a goal: a page or event that counts as a conversion";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: CreateGoal) -> Result<Goal, ApiError> {
        require_project(&cx, input.workspace_id, input.project_id).await?;
        let row = GoalRow {
            id: GoalId::new(),
            workspace_id: input.workspace_id,
            project_id: input.project_id,
            name: input.name.trim().to_owned(),
            kind: input.kind.as_str().to_owned(),
            matches: input.matches.trim().to_owned(),
            created_at: cx.now(),
        };
        let mut tx = cx.write().await?;
        if store::goal_exists(&mut *tx, row.project_id, &row.kind, &row.matches).await? {
            return Err(ApiError::Conflict("a goal already matches this".to_owned()));
        }
        store::insert_goal(&mut *tx, &row).await?;
        tx.commit().await?;
        self.0.sites.clear();
        Ok(goal_to_api(row))
    }
}

struct GoalsDelete(Arc<Ingest>);
impl Operation for GoalsDelete {
    type Input = InGoal;
    type Output = NoContent;
    const ID: &'static str = "analytics.goals.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/goals/{goal_id}";
    const ACCESS: Access = ADMIN;
    const SUMMARY: &'static str = "Delete an analytics goal";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InGoal) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let deleted = store::delete_goal(&mut *tx, input.workspace_id, input.goal_id).await?;
        tx.commit().await?;
        self.0.sites.clear();
        if deleted { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}
