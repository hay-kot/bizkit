//! Funnels: saved step sequences and their results.
//!
//! A person enters with their first step-1 event in the range. Each later
//! step counts when it happens after the previous step and within the
//! funnel's window of the entry. Results read raw events by person key, so
//! cookieless sites link only same-day steps (the visitor hash rotates).

use std::sync::Arc;

use bizkit_core::ApiError;
use bizkit_core::ModuleContext;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{
    Access, Level, Method, NoContent, OpCtx, Operation, Scope as AccessScope, Success,
};
use bizkit_core::time::UnixMillis;
use bizkit_core::validate::{FieldErrors, Validate, check_name};
use futures::{Stream, TryStreamExt as _};
use serde::{Deserialize, Serialize};
use sqlx::SqliteExecutor;
use utoipa::ToSchema;

use crate::api::{Day, FunnelId, GoalKind};
use crate::files::Files;
use crate::names;
use crate::report::Scope;
use crate::stats::{check_range, site_scope};

const READ: Access = Access::Workspace(AccessScope::new("analytics", Level::Read));
const WRITE: Access = Access::Workspace(AccessScope::new("analytics", Level::Write));
const TAGS: &[&str] = &["analytics"];
/// Steps per funnel.
pub(crate) const STEPS: std::ops::RangeInclusive<usize> = 2..=8;
const DEFAULT_WINDOW_DAYS: i64 = 7;
const DAY_MS: i64 = 86_400_000;

pub(crate) fn register(cx: &mut ModuleContext<'_>, files: &Arc<Files>) -> anyhow::Result<()> {
    cx.op(FunnelsList)?;
    cx.op(FunnelsCreate)?;
    cx.op(FunnelsGet)?;
    cx.op(FunnelsUpdate)?;
    cx.op(FunnelsDelete)?;
    cx.op(FunnelsResults(Arc::clone(files)))
}

/// One funnel step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Step {
    /// A pageview or a custom event.
    pub(crate) kind: GoalKind,
    /// A page path (exact, or a prefix ending in `*`) or an event name.
    #[serde(rename = "match")]
    pub(crate) matches: String,
}

impl Step {
    fn is_valid(&self) -> bool {
        match self.kind {
            GoalKind::Page => {
                let path = self.matches.strip_suffix('*').unwrap_or(&self.matches);
                names::is_goal_path(path)
            }
            GoalKind::Event => names::is_event_name(&self.matches),
        }
    }

    fn matches(&self, name: &str, path: Option<&str>) -> bool {
        match self.kind {
            GoalKind::Event => name == self.matches,
            GoalKind::Page => {
                name == crate::beacon::PAGEVIEW
                    && path.is_some_and(|path| {
                        self.matches
                            .strip_suffix('*')
                            .map_or_else(|| path == self.matches, |prefix| path.starts_with(prefix))
                    })
            }
        }
    }
}

/// A saved funnel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct Funnel {
    /// The funnel ID.
    pub(crate) id: FunnelId,
    /// The project.
    pub(crate) project_id: ProjectId,
    /// The display name.
    pub(crate) name: String,
    /// 2 to 8 steps, in order.
    pub(crate) steps: Vec<Step>,
    /// Days after the first step in which the rest must happen.
    pub(crate) window_days: i64,
    /// When it was made.
    pub(crate) created_at: UnixMillis,
    /// When it last changed.
    pub(crate) updated_at: UnixMillis,
}

/// A project's funnels.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct FunnelList {
    /// Sorted by name.
    pub(crate) items: Vec<Funnel>,
}

/// One step's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct StepResult {
    /// The step.
    pub(crate) step: Step,
    /// People who reached it.
    pub(crate) people: i64,
    /// Share of the previous step's people, 0 to 1; 1 for the first step.
    pub(crate) from_previous: f64,
    /// Share of the first step's people, 0 to 1.
    pub(crate) from_first: f64,
}

/// A funnel's results over a range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct FunnelResults {
    /// The funnel.
    pub(crate) funnel: Funnel,
    /// One entry per step.
    pub(crate) steps: Vec<StepResult>,
}

fn check_steps(errors: &mut FieldErrors, steps: &[Step]) {
    if !STEPS.contains(&steps.len()) {
        errors.add("steps", "must have 2 to 8 steps");
    }
    for (at, step) in steps.iter().enumerate() {
        if !step.is_valid() {
            let rule = match step.kind {
                GoalKind::Page => "must be a path that starts with / and may end with *",
                GoalKind::Event => names::EVENT_NAME_RULE,
            };
            errors.add(format!("steps[{at}].match"), rule);
        }
    }
}

fn check_window(errors: &mut FieldErrors, window_days: Option<i64>) {
    if window_days.is_some_and(|days| !(1..=90).contains(&days)) {
        errors.add("window_days", "must be 1 to 90");
    }
}

/// Which funnels to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListFunnels {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
}
impl Validate for ListFunnels {}

/// A new funnel.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateFunnel {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The display name.
    name: String,
    /// 2 to 8 steps, in order.
    steps: Vec<Step>,
    /// Days after the first step in which the rest must happen, 1-90.
    /// Default 7.
    #[serde(default)]
    window_days: Option<i64>,
}

impl Validate for CreateFunnel {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        check_steps(&mut errors, &self.steps);
        check_window(&mut errors, self.window_days);
        errors.into_result()
    }
}

/// A funnel of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InFunnel {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The funnel.
    funnel_id: FunnelId,
}
impl Validate for InFunnel {}

/// Changes to a funnel. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateFunnel {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The funnel.
    funnel_id: FunnelId,
    /// The display name.
    #[serde(default)]
    name: Option<String>,
    /// 2 to 8 steps, in order.
    #[serde(default)]
    steps: Option<Vec<Step>>,
    /// Days after the first step in which the rest must happen, 1-90.
    #[serde(default)]
    window_days: Option<i64>,
}

impl Validate for UpdateFunnel {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(name) = &self.name {
            check_name(&mut errors, "name", name);
        }
        if let Some(steps) = &self.steps {
            check_steps(&mut errors, steps);
        }
        check_window(&mut errors, self.window_days);
        errors.into_result()
    }
}

/// A funnel's results over a range.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FunnelRange {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The funnel.
    funnel_id: FunnelId,
    /// The first day people may enter, UTC.
    from: Day,
    /// The last day people may enter, UTC, inclusive.
    to: Day,
}

impl Validate for FunnelRange {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_range(&mut errors, self.from, self.to);
        errors.into_result()
    }
}

// ---------------------------------------------------------------- store

#[derive(Debug, Clone)]
pub(crate) struct FunnelRow {
    pub(crate) id: FunnelId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) name: String,
    pub(crate) steps: String,
    pub(crate) window_days: i64,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

fn to_api(row: FunnelRow) -> Result<Funnel, ApiError> {
    let steps = serde_json::from_str(&row.steps).map_err(anyhow::Error::from)?;
    Ok(Funnel {
        id: row.id,
        project_id: row.project_id,
        name: row.name,
        steps,
        window_days: row.window_days,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

async fn find(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    funnel_id: FunnelId,
) -> Result<FunnelRow, ApiError> {
    sqlx::query_as!(
        FunnelRow,
        r#"SELECT id AS "id: FunnelId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, steps, window_days,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM analytics_funnels WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        funnel_id,
    )
    .fetch_optional(exec)
    .await?
    .ok_or(ApiError::NotFound)
}

pub(crate) async fn save(exec: impl SqliteExecutor<'_>, row: &FunnelRow) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO analytics_funnels (id, workspace_id, project_id, name, steps, window_days, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET name = excluded.name, steps = excluded.steps,
           window_days = excluded.window_days, updated_at = excluded.updated_at",
        row.id,
        row.workspace_id,
        row.project_id,
        row.name,
        row.steps,
        row.window_days,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

fn encode_steps(steps: &[Step]) -> Result<String, ApiError> {
    let stored: Vec<Step> = steps
        .iter()
        .map(|step| Step {
            kind: step.kind,
            matches: match step.kind {
                GoalKind::Page => names::stored_path(&step.matches),
                GoalKind::Event => step.matches.clone(),
            },
        })
        .collect();
    Ok(serde_json::to_string(&stored).map_err(anyhow::Error::from)?)
}

// ---------------------------------------------------------------- ops

struct FunnelsList;
impl Operation for FunnelsList {
    type Input = ListFunnels;
    type Output = FunnelList;
    const ID: &'static str = "analytics.funnels.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/funnels";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List a project's funnels";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListFunnels) -> Result<FunnelList, ApiError> {
        let rows = sqlx::query_as!(
            FunnelRow,
            r#"SELECT id AS "id: FunnelId", workspace_id AS "workspace_id: WorkspaceId",
                      project_id AS "project_id: ProjectId", name, steps, window_days,
                      created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
               FROM analytics_funnels WHERE workspace_id = ? AND project_id = ? ORDER BY name, id"#,
            input.workspace_id,
            input.project_id,
        )
        .fetch_all(cx.platform().db().read())
        .await?;
        Ok(FunnelList { items: rows.into_iter().map(to_api).collect::<Result<_, _>>()? })
    }
}

struct FunnelsCreate;
impl Operation for FunnelsCreate {
    type Input = CreateFunnel;
    type Output = Funnel;
    const ID: &'static str = "analytics.funnels.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/funnels";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Save a funnel: 2 to 8 page or event steps";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateFunnel) -> Result<Funnel, ApiError> {
        site_scope(cx.platform().db(), input.workspace_id, input.project_id).await?;
        let now = cx.now();
        let row = FunnelRow {
            id: FunnelId::new(),
            workspace_id: input.workspace_id,
            project_id: input.project_id,
            name: input.name.trim().to_owned(),
            steps: encode_steps(&input.steps)?,
            window_days: input.window_days.unwrap_or(DEFAULT_WINDOW_DAYS),
            created_at: now,
            updated_at: now,
        };
        let mut tx = cx.write().await?;
        save(&mut *tx, &row).await?;
        tx.commit().await?;
        to_api(row)
    }
}

struct FunnelsGet;
impl Operation for FunnelsGet {
    type Input = InFunnel;
    type Output = Funnel;
    const ID: &'static str = "analytics.funnels.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/funnels/{funnel_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a funnel";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InFunnel) -> Result<Funnel, ApiError> {
        to_api(find(cx.platform().db().read(), input.workspace_id, input.funnel_id).await?)
    }
}

struct FunnelsUpdate;
impl Operation for FunnelsUpdate {
    type Input = UpdateFunnel;
    type Output = Funnel;
    const ID: &'static str = "analytics.funnels.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/funnels/{funnel_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Change a funnel";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateFunnel) -> Result<Funnel, ApiError> {
        let mut tx = cx.write().await?;
        let mut row = find(&mut *tx, input.workspace_id, input.funnel_id).await?;
        if let Some(name) = input.name {
            name.trim().clone_into(&mut row.name);
        }
        if let Some(steps) = input.steps {
            row.steps = encode_steps(&steps)?;
        }
        if let Some(days) = input.window_days {
            row.window_days = days;
        }
        row.updated_at = cx.now();
        save(&mut *tx, &row).await?;
        tx.commit().await?;
        to_api(row)
    }
}

struct FunnelsDelete;
impl Operation for FunnelsDelete {
    type Input = InFunnel;
    type Output = NoContent;
    const ID: &'static str = "analytics.funnels.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/funnels/{funnel_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Delete a funnel";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InFunnel) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let result = sqlx::query!(
            "DELETE FROM analytics_funnels WHERE workspace_id = ? AND id = ?",
            input.workspace_id,
            input.funnel_id,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if result.rows_affected() == 1 { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

struct FunnelsResults(Arc<Files>);
impl Operation for FunnelsResults {
    type Input = FunnelRange;
    type Output = FunnelResults;
    const ID: &'static str = "analytics.funnels.results";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/funnels/{funnel_id}/results";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str =
        "Count the people who reached each step of a funnel, with conversion between steps";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: FunnelRange) -> Result<FunnelResults, ApiError> {
        let db = cx.platform().db();
        let funnel = to_api(find(db.read(), input.workspace_id, input.funnel_id).await?)?;
        let scope = Scope { workspace_id: input.workspace_id, project_id: funnel.project_id };
        let file = self.0.for_op(cx.platform(), scope).await?;
        let last = input.to.plus_days(funnel.window_days);
        let entry_end = input.to.next().start().0;
        let window = funnel.window_days * DAY_MS;
        let mut counts = vec![0_i64; funnel.steps.len()];
        let mut person: Vec<StepEvent> = Vec::new();
        let mut rows = step_events(file.db.read(), scope, input.from, last, &funnel.steps);
        while let Some(event) = rows.try_next().await? {
            if person.last().is_some_and(|previous| previous.person != event.person) {
                count_person(&mut counts, &funnel.steps, &person, entry_end, window);
                person.clear();
            }
            person.push(event);
        }
        count_person(&mut counts, &funnel.steps, &person, entry_end, window);
        let steps = results(&funnel.steps, &counts);
        Ok(FunnelResults { funnel, steps })
    }
}

/// An event that matches at least one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StepEvent {
    pub(crate) person: String,
    pub(crate) ts: i64,
    pub(crate) name: String,
    pub(crate) path: Option<String>,
}

/// Events from `from` through `to` that could match a step, by person and
/// time, as a stream: a busy site has too many to hold at once. Only the
/// names the steps use are read, and pageviews only on the steps' paths.
fn step_events<'e>(
    exec: impl SqliteExecutor<'e> + 'e,
    scope: Scope,
    from: Day,
    to: Day,
    steps: &[Step],
) -> impl Stream<Item = sqlx::Result<StepEvent>> + 'e {
    let names: Vec<&str> = steps
        .iter()
        .map(|step| match step.kind {
            GoalKind::Page => crate::beacon::PAGEVIEW,
            GoalKind::Event => step.matches.as_str(),
        })
        .collect();
    let paths: Vec<&str> = steps
        .iter()
        .filter(|step| step.kind == GoalKind::Page)
        .map(|step| step.matches.as_str())
        .collect();
    let names = serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_owned());
    let paths = serde_json::to_string(&paths).unwrap_or_else(|_| "[]".to_owned());
    let (from, to) = (from.to_string(), to.to_string());
    sqlx::query_as!(
        StepEvent,
        r#"SELECT e.person, e.ts AS "ts!: i64", e.name, e.path FROM analytics_events AS e
           WHERE e.workspace_id = ? AND e.project_id = ? AND e.day >= ? AND e.day <= ?
             AND e.name IN (SELECT value FROM json_each(?))
             AND (e.name <> 'pageview' OR EXISTS (
               SELECT 1 FROM json_each(?) AS step
               WHERE e.path = step.value
                  OR (substr(step.value, -1) = '*'
                      AND substr(e.path, 1, length(step.value) - 1) = substr(step.value, 1, length(step.value) - 1))))
           ORDER BY e.person, e.ts, e.id"#,
        scope.workspace_id,
        scope.project_id,
        from,
        to,
        names,
        paths,
    )
    .fetch(exec)
}

/// Adds one person's events, sorted by time, to the step counts.
fn count_person(
    counts: &mut [i64],
    steps: &[Step],
    events: &[StepEvent],
    entry_end: i64,
    window: i64,
) {
    let depth = depth(steps, events, entry_end, window);
    for count in counts.iter_mut().take(depth) {
        *count += 1;
    }
}

/// The number of steps one person reached.
fn depth(steps: &[Step], events: &[StepEvent], entry_end: i64, window: i64) -> usize {
    let Some(first) = steps.first() else { return 0 };
    let Some(entry) =
        events.iter().position(|e| e.ts < entry_end && first.matches(&e.name, e.path.as_deref()))
    else {
        return 0;
    };
    let deadline = events.get(entry).map_or(0, |e| e.ts).saturating_add(window);
    let mut reached = 1;
    let mut after = entry;
    for step in steps.iter().skip(1) {
        let next = events
            .iter()
            .enumerate()
            .skip(after + 1)
            .take_while(|(_, e)| e.ts <= deadline)
            .find(|(_, e)| step.matches(&e.name, e.path.as_deref()));
        let Some((at, _)) = next else { break };
        reached += 1;
        after = at;
    }
    reached
}

fn share(part: i64, whole: i64) -> f64 {
    let as_f64 = |n: i64| f64::from(u32::try_from(n).unwrap_or(u32::MAX));
    if whole > 0 { as_f64(part) / as_f64(whole) } else { 0.0 }
}

fn results(steps: &[Step], reached: &[i64]) -> Vec<StepResult> {
    let first = reached.first().copied().unwrap_or(0);
    steps
        .iter()
        .zip(reached)
        .enumerate()
        .map(|(at, (step, &people))| {
            let previous = at.checked_sub(1).and_then(|i| reached.get(i)).copied();
            StepResult {
                step: step.clone(),
                people,
                from_previous: previous.map_or(1.0, |previous| share(people, previous)),
                from_first: share(people, first),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many people reached each step. `events` are sorted by person and
    /// time; people enter on their first step-1 event before `entry_end`.
    fn reached(steps: &[Step], events: &[StepEvent], entry_end: i64, window: i64) -> Vec<i64> {
        let mut counts = vec![0_i64; steps.len()];
        for person in events.chunk_by(|a, b| a.person == b.person) {
            count_person(&mut counts, steps, person, entry_end, window);
        }
        counts
    }

    fn page(path: &str) -> Step {
        Step { kind: GoalKind::Page, matches: path.to_owned() }
    }

    fn event(name: &str) -> Step {
        Step { kind: GoalKind::Event, matches: name.to_owned() }
    }

    fn at(person: &str, ts: i64, name: &str, path: Option<&str>) -> StepEvent {
        StepEvent {
            person: person.to_owned(),
            ts,
            name: name.to_owned(),
            path: path.map(str::to_owned),
        }
    }

    #[test]
    fn steps_must_follow_in_order_within_the_window() {
        let steps = [page("/pricing"), event("signup_started"), event("signup")];
        let events = [
            // a: all three in order.
            at("a", 1, "pageview", Some("/pricing")),
            at("a", 2, "signup_started", None),
            at("a", 3, "signup", None),
            // b: signup before signup_started counts only the first two.
            at("b", 1, "pageview", Some("/pricing")),
            at("b", 2, "signup", None),
            at("b", 3, "signup_started", None),
            // c: the second step comes after the window.
            at("c", 1, "pageview", Some("/pricing")),
            at("c", 200, "signup_started", None),
            // d: entered after the range.
            at("d", 500, "pageview", Some("/pricing")),
            at("d", 501, "signup_started", None),
        ];
        assert_eq!(reached(&steps, &events, 100, 100), [3, 2, 1]);
    }

    #[test]
    fn page_steps_match_exactly_or_by_prefix() {
        assert!(page("/blog/*").matches("pageview", Some("/blog/launch")));
        assert!(!page("/blog/*").matches("pageview", Some("/pricing")));
        assert!(!page("/blog").matches("pageview", Some("/blog/launch")));
        assert!(!page("/blog").matches("blog", Some("/blog")), "only pageviews");
        assert!(page("/blog/*").is_valid());
        assert!(!page("/blog/*/x").is_valid());
    }

    #[test]
    fn shares() {
        let steps = [page("/"), event("a"), event("b")];
        let out = results(&steps, &[4, 2, 0]);
        assert_eq!(
            out.iter().map(|s| (s.people, s.from_previous, s.from_first)).collect::<Vec<_>>(),
            [(4, 1.0, 1.0), (2, 0.5, 0.5), (0, 0.0, 0.0)]
        );
    }
}
