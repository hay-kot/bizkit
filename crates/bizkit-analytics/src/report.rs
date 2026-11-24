//! Web report queries. A range reads `analytics_daily` for the days the
//! rollup has closed and aggregates raw events for the rest, in the same
//! shape, so today's numbers are live and old ranges stay fast.
//!
//! Every raw aggregate picks its value with a `CASE` on the dimension
//! instead of building SQL, so the queries stay checked at compile time.
//! Days are compared as `YYYY-MM-DD` text, which sorts like dates.

use std::collections::HashMap;

use bizkit_core::ids::{ProjectId, WorkspaceId};
use sqlx::SqliteExecutor;

use crate::api::{Day, GoalId};
use crate::store::GoalRow;

/// Which days of a range come from rollups and which from raw events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Split {
    pub(crate) rolled: Option<(Day, Day)>,
    pub(crate) raw: Option<(Day, Day)>,
}

/// Splits `from..=to` at `rolled_through`, the last day the rollup closed.
pub(crate) fn split(from: Day, to: Day, rolled_through: Option<Day>) -> Split {
    let Some(rolled) = rolled_through else { return Split { rolled: None, raw: Some((from, to)) } };
    let rolled_part = (from <= rolled).then(|| (from, to.min(rolled)));
    let raw_from = from.max(rolled.next());
    let raw_part = (raw_from <= to && rolled < to).then_some((raw_from, to));
    Split { rolled: rolled_part, raw: raw_part }
}

/// One value's counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) visitors: i64,
    pub(crate) pageviews: i64,
    pub(crate) events: i64,
}

impl Counts {
    const fn add(&mut self, other: Self) {
        self.visitors += other.visitors;
        self.pageviews += other.pageviews;
        self.events += other.events;
    }
}

/// The scope every report query filters on.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scope {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
}

/// Counts per value of `dimension` (`total` for one row with value `''`).
pub(crate) async fn by_value<'e>(
    exec: impl SqliteExecutor<'e> + Copy,
    scope: Scope,
    dimension: &str,
    split: Split,
) -> sqlx::Result<HashMap<String, Counts>> {
    let mut out: HashMap<String, Counts> = HashMap::new();
    if let Some((from, to)) = split.rolled {
        let (from, to) = (from.to_string(), to.to_string());
        let rows = sqlx::query!(
            r#"SELECT value, SUM(visitors) AS "visitors!: i64", SUM(pageviews) AS "pageviews!: i64",
                      SUM(events) AS "events!: i64"
               FROM analytics_daily
               WHERE workspace_id = ? AND project_id = ? AND dimension = ? AND day >= ? AND day <= ?
               GROUP BY value"#,
            scope.workspace_id,
            scope.project_id,
            dimension,
            from,
            to,
        )
        .fetch_all(exec)
        .await?;
        for row in rows {
            let counts =
                Counts { visitors: row.visitors, pageviews: row.pageviews, events: row.events };
            out.entry(row.value).or_default().add(counts);
        }
    }
    if let Some((from, to)) = split.raw {
        let (from, to) = (from.to_string(), to.to_string());
        let rows = sqlx::query!(
            r#"SELECT value AS "value!: String", SUM(visitors) AS "visitors!: i64",
                      SUM(pageviews) AS "pageviews!: i64", SUM(events) AS "events!: i64"
               FROM (
                 SELECT day,
                        COALESCE(CASE ? WHEN 'page' THEN path WHEN 'referrer' THEN referrer_host
                                        WHEN 'utm_source' THEN utm_source WHEN 'utm_medium' THEN utm_medium
                                        WHEN 'utm_campaign' THEN utm_campaign WHEN 'country' THEN country
                                        WHEN 'device' THEN device WHEN 'browser' THEN browser
                                        WHEN 'os' THEN os WHEN 'event' THEN name END, '') AS value,
                        COUNT(DISTINCT visitor) AS visitors,
                        SUM(name = 'pageview') AS pageviews,
                        SUM(name <> 'pageview') AS events
                 FROM analytics_events
                 WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ?
                   AND CASE ? WHEN 'total' THEN 1 WHEN 'event' THEN name <> 'pageview' ELSE name = 'pageview' END
                 GROUP BY day, value)
               GROUP BY value"#,
            dimension,
            scope.workspace_id,
            scope.project_id,
            from,
            to,
            dimension,
        )
        .fetch_all(exec)
        .await?;
        for row in rows {
            let counts =
                Counts { visitors: row.visitors, pageviews: row.pageviews, events: row.events };
            out.entry(row.value).or_default().add(counts);
        }
    }
    Ok(out)
}

/// Visitors and pageviews per day, for days with any event.
pub(crate) async fn daily<'e>(
    exec: impl SqliteExecutor<'e> + Copy,
    scope: Scope,
    split: Split,
) -> sqlx::Result<HashMap<String, (i64, i64)>> {
    let mut out = HashMap::new();
    if let Some((from, to)) = split.rolled {
        let (from, to) = (from.to_string(), to.to_string());
        let rows = sqlx::query!(
            "SELECT day, visitors, pageviews FROM analytics_daily
             WHERE workspace_id = ? AND project_id = ? AND dimension = 'total' AND day >= ? AND day <= ?",
            scope.workspace_id,
            scope.project_id,
            from,
            to,
        )
        .fetch_all(exec)
        .await?;
        out.extend(rows.into_iter().map(|row| (row.day, (row.visitors, row.pageviews))));
    }
    if let Some((from, to)) = split.raw {
        let (from, to) = (from.to_string(), to.to_string());
        let rows = sqlx::query!(
            r#"SELECT day, COUNT(DISTINCT visitor) AS "visitors!: i64",
                      SUM(name = 'pageview') AS "pageviews!: i64"
               FROM analytics_events
               WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ?
               GROUP BY day"#,
            scope.workspace_id,
            scope.project_id,
            from,
            to,
        )
        .fetch_all(exec)
        .await?;
        out.extend(rows.into_iter().map(|row| (row.day, (row.visitors, row.pageviews))));
    }
    Ok(out)
}

/// Visitors and pageviews per hour of `day` (0 to 23), from raw events.
pub(crate) async fn hourly(
    exec: impl SqliteExecutor<'_>,
    scope: Scope,
    day: Day,
) -> sqlx::Result<HashMap<i64, (i64, i64)>> {
    let (start, text) = (day.start(), day.to_string());
    let rows = sqlx::query!(
        r#"SELECT (ts - ?) / 3600000 AS "hour!: i64", COUNT(DISTINCT visitor) AS "visitors!: i64",
                  SUM(name = 'pageview') AS "pageviews!: i64"
           FROM analytics_events
           WHERE workspace_id = ? AND project_id = ? AND day = ?
           GROUP BY 1"#,
        start,
        scope.workspace_id,
        scope.project_id,
        text,
    )
    .fetch_all(exec)
    .await?;
    Ok(rows.into_iter().map(|row| (row.hour, (row.visitors, row.pageviews))).collect())
}

/// Conversions per goal: unique visitors per day who viewed the goal's page
/// or sent its event. `goals` come from the app database; the counts from
/// the project file `exec`.
pub(crate) async fn goal_conversions<'e>(
    exec: impl SqliteExecutor<'e> + Copy,
    scope: Scope,
    goals: &[GoalRow],
    split: Split,
) -> sqlx::Result<HashMap<GoalId, i64>> {
    let mut out: HashMap<GoalId, i64> = HashMap::new();
    for goal in goals {
        let mut visitors = 0;
        if let Some((from, to)) = split.rolled {
            let (from, to) = (from.to_string(), to.to_string());
            visitors += sqlx::query_scalar!(
                r#"SELECT COALESCE(SUM(visitors), 0) AS "visitors!: i64" FROM analytics_daily
                   WHERE workspace_id = ? AND project_id = ? AND dimension = ? AND value = ?
                     AND day >= ? AND day <= ?"#,
                scope.workspace_id,
                scope.project_id,
                goal.kind,
                goal.matches,
                from,
                to,
            )
            .fetch_one(exec)
            .await?;
        }
        if let Some((from, to)) = split.raw {
            let (from, to) = (from.to_string(), to.to_string());
            visitors += sqlx::query_scalar!(
                r#"SELECT COUNT(DISTINCT day || ':' || visitor) AS "visitors!: i64"
                   FROM analytics_events
                   WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ?
                     AND ((? = 'page' AND name = 'pageview' AND path = ?)
                       OR (? = 'event' AND name = ?))"#,
                scope.workspace_id,
                scope.project_id,
                from,
                to,
                goal.kind,
                goal.matches,
                goal.kind,
                goal.matches,
            )
            .fetch_one(exec)
            .await?;
        }
        if visitors > 0 {
            out.insert(goal.id, visitors);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> Day {
        text.parse().unwrap()
    }

    #[test]
    fn splits_at_the_rolled_day() {
        let (from, to) = (day("2026-01-01"), day("2026-01-10"));
        assert_eq!(split(from, to, None), Split { rolled: None, raw: Some((from, to)) });
        assert_eq!(
            split(from, to, Some(day("2026-01-05"))),
            Split { rolled: Some((from, day("2026-01-05"))), raw: Some((day("2026-01-06"), to)) }
        );
        assert_eq!(split(from, to, Some(to)), Split { rolled: Some((from, to)), raw: None });
        assert_eq!(
            split(from, to, Some(day("2026-02-01"))),
            Split { rolled: Some((from, to)), raw: None }
        );
        assert_eq!(
            split(from, to, Some(day("2025-12-01"))),
            Split { rolled: None, raw: Some((from, to)) }
        );
    }
}
