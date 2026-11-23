//! The web report operations: `stats.summary` for the Overview's tiles and
//! chart, and `stats.breakdown` for its ranked panels.

use std::cmp::Reverse;

use bizkit_core::ApiError;
use bizkit_core::ModuleContext;
use bizkit_core::db::Db;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Level, Method, OpCtx, Operation, Scope as AccessScope};
use bizkit_core::time::UnixMillis;
use bizkit_core::validate::{FieldErrors, Validate};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::api::{Breakdown, BreakdownRow, Day, Dimension, Interval, SeriesPoint, Summary, Totals};
use crate::report::{self, Scope, Split};
use crate::store;

const READ: Access = Access::Workspace(AccessScope::new("analytics", Level::Read));
const TAGS: &[&str] = &["analytics"];
/// The longest report range, in days.
pub(crate) const MAX_RANGE_DAYS: i64 = 400;
const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 1_000;
const HOUR_MS: i64 = 3_600_000;

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.op(StatsSummary)?;
    cx.op(StatsBreakdown)
}

/// Adds errors for a range that is reversed or too long.
pub(crate) fn check_range(errors: &mut FieldErrors, from: Day, to: Day) {
    if to < from {
        errors.add("to", "must be on or after from");
    } else if from.days_until(to) >= MAX_RANGE_DAYS {
        errors.add("to", format!("must be within {MAX_RANGE_DAYS} days of from"));
    }
}

/// The site's report scope and how far its rollup has closed days.
pub(crate) async fn site_scope(
    db: &Db,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> Result<(Scope, Option<Day>), ApiError> {
    let site =
        store::find_site(db.read(), workspace_id, project_id).await?.ok_or(ApiError::NotFound)?;
    let rolled = site.rolled_through.as_deref().and_then(|day| day.parse().ok());
    Ok((Scope { workspace_id, project_id }, rolled))
}

/// The Overview's numbers for a range.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SummaryQuery {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The first day, UTC.
    from: Day,
    /// The last day, UTC, inclusive. At most 400 days after `from`.
    to: Day,
    /// Also count the previous period of the same length.
    #[serde(default)]
    compare: bool,
}

impl Validate for SummaryQuery {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_range(&mut errors, self.from, self.to);
        errors.into_result()
    }
}

/// A ranked breakdown for a range.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BreakdownQuery {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The first day, UTC.
    from: Day,
    /// The last day, UTC, inclusive. At most 400 days after `from`.
    to: Day,
    /// What to group by.
    dimension: Dimension,
    /// The most rows, 1 to 1000; default 10.
    #[serde(default)]
    limit: Option<usize>,
}

impl Validate for BreakdownQuery {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_range(&mut errors, self.from, self.to);
        if self.limit.is_some_and(|limit| !(1..=MAX_LIMIT).contains(&limit)) {
            errors.add("limit", format!("must be 1 to {MAX_LIMIT}"));
        }
        errors.into_result()
    }
}

struct StatsSummary;
impl Operation for StatsSummary {
    type Input = SummaryQuery;
    type Output = Summary;
    const ID: &'static str = "analytics.stats.summary";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/stats/summary";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str =
        "Count visitors, pageviews, custom events and conversions over a date range";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: SummaryQuery) -> Result<Summary, ApiError> {
        let db = cx.platform().db();
        let (scope, rolled) = site_scope(db, input.workspace_id, input.project_id).await?;
        let (from, to) = (input.from, input.to);
        let current = totals(db, scope, report::split(from, to, rolled)).await?;
        let points = series(db, scope, from, to, rolled).await?;
        let (previous, previous_series) = if input.compare {
            let days = from.days_until(to) + 1;
            let (prev_from, prev_to) = (from.plus_days(-days), from.previous());
            let prev_totals = totals(db, scope, report::split(prev_from, prev_to, rolled)).await?;
            (Some(prev_totals), Some(series(db, scope, prev_from, prev_to, rolled).await?))
        } else {
            (None, None)
        };
        let interval = if from == to { Interval::Hour } else { Interval::Day };
        Ok(Summary { totals: current, previous, interval, series: points, previous_series })
    }
}

async fn totals(db: &Db, scope: Scope, split: Split) -> Result<Totals, ApiError> {
    let by_value = report::by_value(db.read(), scope, "total", split).await?;
    let counts = by_value.get("").copied().unwrap_or_default();
    let conversions = report::goal_conversions(db.read(), scope, split).await?.values().sum();
    Ok(Totals {
        visitors: counts.visitors,
        pageviews: counts.pageviews,
        events: counts.events,
        conversions,
    })
}

/// One point per hour for a one-day range, else one per day; days and
/// hours with no events are zero.
async fn series(
    db: &Db,
    scope: Scope,
    from: Day,
    to: Day,
    rolled: Option<Day>,
) -> Result<Vec<SeriesPoint>, ApiError> {
    if from == to {
        let hours = report::hourly(db.read(), scope, from).await?;
        return Ok((0..24)
            .map(|hour| {
                let (visitors, pageviews) = hours.get(&hour).copied().unwrap_or_default();
                let start = UnixMillis(from.start().0 + hour * HOUR_MS);
                SeriesPoint { start, visitors, pageviews }
            })
            .collect());
    }
    let days = report::daily(db.read(), scope, report::split(from, to, rolled)).await?;
    let mut points = Vec::new();
    let mut day = from;
    while day <= to {
        let (visitors, pageviews) = days.get(&day.to_string()).copied().unwrap_or_default();
        points.push(SeriesPoint { start: day.start(), visitors, pageviews });
        day = day.next();
    }
    Ok(points)
}

struct StatsBreakdown;
impl Operation for StatsBreakdown {
    type Input = BreakdownQuery;
    type Output = Breakdown;
    const ID: &'static str = "analytics.stats.breakdown";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/stats/breakdown";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str =
        "Rank pages, sources, countries, devices, events or goals over a date range";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: BreakdownQuery) -> Result<Breakdown, ApiError> {
        let db = cx.platform().db();
        let (scope, rolled) = site_scope(db, input.workspace_id, input.project_id).await?;
        let split = report::split(input.from, input.to, rolled);
        let mut items = if input.dimension == Dimension::Goal {
            goal_rows(db, scope, split).await?
        } else {
            report::by_value(db.read(), scope, input.dimension.as_str(), split)
                .await?
                .into_iter()
                .map(|(value, counts)| BreakdownRow {
                    value,
                    visitors: counts.visitors,
                    pageviews: counts.pageviews,
                    events: counts.events,
                    goal_id: None,
                    conversion_rate: None,
                })
                .collect()
        };
        items.sort_by(|a, b| {
            (Reverse(a.visitors), Reverse(a.pageviews + a.events), &a.value).cmp(&(
                Reverse(b.visitors),
                Reverse(b.pageviews + b.events),
                &b.value,
            ))
        });
        items.truncate(input.limit.unwrap_or(DEFAULT_LIMIT));
        Ok(Breakdown { items })
    }
}

/// Every goal, including those with no conversions in the range.
async fn goal_rows(db: &Db, scope: Scope, split: Split) -> Result<Vec<BreakdownRow>, ApiError> {
    let goals = store::list_goals(db.read(), scope.workspace_id, scope.project_id).await?;
    let conversions = report::goal_conversions(db.read(), scope, split).await?;
    let total = report::by_value(db.read(), scope, "total", split).await?;
    let visitors = total.get("").map_or(0, |counts| counts.visitors);
    Ok(goals
        .into_iter()
        .map(|goal| {
            let converted = conversions.get(&goal.id).copied().unwrap_or(0);
            let as_f64 = |n: i64| f64::from(u32::try_from(n).unwrap_or(u32::MAX));
            let rate = if visitors > 0 { as_f64(converted) / as_f64(visitors) } else { 0.0 };
            BreakdownRow {
                value: goal.name,
                visitors: converted,
                pageviews: 0,
                events: 0,
                goal_id: Some(goal.id),
                conversion_rate: Some(rate.min(1.0)),
            }
        })
        .collect())
}
