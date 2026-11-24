//! Runs an [`InsightQuery`] against a project file.
//!
//! Every query is static SQL. Filters travel as one JSON array (see
//! [`encode_filters`]) matched with `json_each`, and choices such as the
//! breakdown source are `CASE` branches on a parameter, so sqlx checks all
//! of it at compile time and nothing in a query object reaches SQL text.
//!
//! Person keys: `p:` identified (or aliased to an identified person), `a:`
//! an anonymous ID with no identify yet, `v:` a cookieless visitor, which
//! changes every day. `users` counts every key, as the explorer does;
//! `share` and `per_user` leave `v:` out, as retention does, because a
//! cookieless visitor can't be followed across days.

use bizkit_core::time::UnixMillis;
use sqlx::SqlitePool;

use super::query::{
    BreakdownBy, BreakdownSource, InsightKind, InsightPoint, InsightQuery, InsightResult,
    InsightRow, Measure, TIMES_CAP, TOP_VALUES, TimesBucket, encode_filters,
};
use crate::api::{Day, Interval};
use crate::report::Scope;

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;

/// The range an insight runs over.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Range {
    pub(crate) from: Day,
    pub(crate) to: Day,
    /// The oldest day raw events may still have; earlier ones are pruned.
    pub(crate) oldest_raw: Day,
}

impl Range {
    fn raw_since(self) -> Option<Day> {
        (self.from < self.oldest_raw).then_some(self.oldest_raw)
    }

    fn hourly(self) -> bool {
        self.from == self.to
    }

    fn bucket_ms(self) -> i64 {
        if self.hourly() { HOUR_MS } else { DAY_MS }
    }

    /// Every bucket start in the range.
    fn starts(self) -> Vec<UnixMillis> {
        if self.hourly() {
            return (0..24).map(|hour| UnixMillis(self.from.start().0 + hour * HOUR_MS)).collect();
        }
        let mut starts = Vec::new();
        let mut day = self.from;
        while day <= self.to {
            starts.push(day.start());
            day = day.next();
        }
        starts
    }
}

/// One events query's bound values.
struct Ask<'a> {
    scope: Scope,
    from: String,
    to: String,
    event: &'a str,
    filters: String,
}

/// Runs `query` over `range`.
pub(crate) async fn run(
    pool: &SqlitePool,
    scope: Scope,
    range: Range,
    query: &InsightQuery,
) -> sqlx::Result<InsightResult> {
    let ask = Ask {
        scope,
        from: range.from.to_string(),
        to: range.to.to_string(),
        event: query.event.as_deref().unwrap_or_default(),
        filters: encode_filters(&query.filters),
    };
    match (query.kind, query.measure(), &query.breakdown) {
        (InsightKind::People, _, None) => people_count(pool, &ask).await,
        (InsightKind::People, _, Some(by)) => people_breakdown(pool, &ask, by).await,
        (InsightKind::Events, Measure::PerUser, _) => distribution(pool, &ask, range).await,
        (InsightKind::Events, Measure::Share, None) => share_series(pool, &ask, range).await,
        (InsightKind::Events, measure, None) => series(pool, &ask, range, measure).await,
        (InsightKind::Events, measure, Some(by)) => {
            events_breakdown(pool, &ask, range, measure, by).await
        }
    }
}

fn as_f64(n: i64) -> f64 {
    f64::from(i32::try_from(n).unwrap_or(i32::MAX))
}

fn ratio(part: i64, whole: i64) -> f64 {
    if whole == 0 { 0.0 } else { as_f64(part) / as_f64(whole) }
}

/// Matching events and distinct people per bucket, and over the range.
async fn series(
    pool: &SqlitePool,
    ask: &Ask<'_>,
    range: Range,
    measure: Measure,
) -> sqlx::Result<InsightResult> {
    let (origin, bucket) = (range.from.start(), range.bucket_ms());
    let rows = sqlx::query!(
        r#"SELECT ? + ((ts - ?) / ?) * ? AS "start!: i64", COUNT(*) AS "count!: i64",
                  COUNT(DISTINCT person) AS "users!: i64"
           FROM analytics_events e
           WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ? AND name = ?
             AND NOT EXISTS (SELECT 1 FROM json_each(?) f WHERE NOT (
               CASE json_extract(f.value, '$.op')
               WHEN 'is_not' THEN NOT EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                 WHERE x.key = json_extract(f.value, '$.k')
                   AND (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END))
               ELSE EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                 WHERE x.key = json_extract(f.value, '$.k') AND CASE json_extract(f.value, '$.op')
                   WHEN 'is' THEN (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END)
                   WHEN 'gt' THEN x.type IN ('integer', 'real') AND x.atom > json_extract(f.value, '$.n')
                   WHEN 'lt' THEN x.type IN ('integer', 'real') AND x.atom < json_extract(f.value, '$.n')
                   ELSE x.type <> 'null' END)
               END))
           GROUP BY 1"#,
        origin,
        origin,
        bucket,
        bucket,
        ask.scope.workspace_id,
        ask.scope.project_id,
        ask.from,
        ask.to,
        ask.event,
        ask.filters,
    )
    .fetch_all(pool)
    .await?;
    let (total, users) = totals(pool, ask).await?;
    let points = range
        .starts()
        .into_iter()
        .map(|start| {
            let row = rows.iter().find(|row| row.start == start.0);
            let value =
                row.map_or(0, |row| if measure == Measure::Users { row.users } else { row.count });
            InsightPoint { start, value: as_f64(value) }
        })
        .collect();
    let value = if measure == Measure::Users { users } else { total };
    Ok(InsightResult::Series {
        measure,
        interval: if range.hourly() { Interval::Hour } else { Interval::Day },
        value: as_f64(value),
        active: None,
        points,
        raw_since: range.raw_since(),
    })
}

/// Matching events and distinct people over the range.
async fn totals(pool: &SqlitePool, ask: &Ask<'_>) -> sqlx::Result<(i64, i64)> {
    let row = sqlx::query!(
        r#"SELECT COUNT(*) AS "count!: i64", COUNT(DISTINCT person) AS "users!: i64"
           FROM analytics_events e
           WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ? AND name = ?
             AND NOT EXISTS (SELECT 1 FROM json_each(?) f WHERE NOT (
               CASE json_extract(f.value, '$.op')
               WHEN 'is_not' THEN NOT EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                 WHERE x.key = json_extract(f.value, '$.k')
                   AND (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END))
               ELSE EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                 WHERE x.key = json_extract(f.value, '$.k') AND CASE json_extract(f.value, '$.op')
                   WHEN 'is' THEN (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END)
                   WHEN 'gt' THEN x.type IN ('integer', 'real') AND x.atom > json_extract(f.value, '$.n')
                   WHEN 'lt' THEN x.type IN ('integer', 'real') AND x.atom < json_extract(f.value, '$.n')
                   ELSE x.type <> 'null' END)
               END))"#,
        ask.scope.workspace_id,
        ask.scope.project_id,
        ask.from,
        ask.to,
        ask.event,
        ask.filters,
    )
    .fetch_one(pool)
    .await?;
    Ok((row.count, row.users))
}

/// People with a stored ID who did it, and active people, over the range.
async fn share_totals(pool: &SqlitePool, ask: &Ask<'_>) -> sqlx::Result<(i64, i64)> {
    let row = sqlx::query!(
        r#"SELECT COUNT(DISTINCT CASE WHEN name = ? AND NOT EXISTS (SELECT 1 FROM json_each(?) f WHERE NOT (
               CASE json_extract(f.value, '$.op')
               WHEN 'is_not' THEN NOT EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                 WHERE x.key = json_extract(f.value, '$.k')
                   AND (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END))
               ELSE EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                 WHERE x.key = json_extract(f.value, '$.k') AND CASE json_extract(f.value, '$.op')
                   WHEN 'is' THEN (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END)
                   WHEN 'gt' THEN x.type IN ('integer', 'real') AND x.atom > json_extract(f.value, '$.n')
                   WHEN 'lt' THEN x.type IN ('integer', 'real') AND x.atom < json_extract(f.value, '$.n')
                   ELSE x.type <> 'null' END)
               END)) THEN person END) AS "users!: i64",
                  COUNT(DISTINCT person) AS "active!: i64"
           FROM analytics_events e
           WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ?
             AND substr(person, 1, 2) <> 'v:'"#,
        ask.event,
        ask.filters,
        ask.scope.workspace_id,
        ask.scope.project_id,
        ask.from,
        ask.to,
    )
    .fetch_one(pool)
    .await?;
    Ok((row.users, row.active))
}

/// The share of active people who did it, per bucket and over the range.
/// One scan fills a row per person and bucket; SQL counts it twice, by
/// bucket and over the range, and only those counts come back. The range
/// row has no `start`.
async fn share_series(
    pool: &SqlitePool,
    ask: &Ask<'_>,
    range: Range,
) -> sqlx::Result<InsightResult> {
    let (origin, bucket) = (range.from.start(), range.bucket_ms());
    let rows = sqlx::query!(
        r#"WITH b AS MATERIALIZED (
             SELECT ? + ((ts - ?) / ?) * ? AS start, person,
                  MAX(name = ? AND NOT EXISTS (SELECT 1 FROM json_each(?) f WHERE NOT (
               CASE json_extract(f.value, '$.op')
               WHEN 'is_not' THEN NOT EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                 WHERE x.key = json_extract(f.value, '$.k')
                   AND (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END))
               ELSE EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                 WHERE x.key = json_extract(f.value, '$.k') AND CASE json_extract(f.value, '$.op')
                   WHEN 'is' THEN (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END)
                   WHEN 'gt' THEN x.type IN ('integer', 'real') AND x.atom > json_extract(f.value, '$.n')
                   WHEN 'lt' THEN x.type IN ('integer', 'real') AND x.atom < json_extract(f.value, '$.n')
                   ELSE x.type <> 'null' END)
               END))) AS did
           FROM analytics_events e
           WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ?
             AND substr(person, 1, 2) <> 'v:'
           GROUP BY 1, 2)
           SELECT start AS "start?: i64", SUM(did) AS "users!: i64", COUNT(*) AS "active!: i64"
           FROM b GROUP BY start
           UNION ALL
           SELECT NULL, COUNT(DISTINCT CASE WHEN did THEN person END), COUNT(DISTINCT person) FROM b"#,
        origin,
        origin,
        bucket,
        bucket,
        ask.event,
        ask.filters,
        ask.scope.workspace_id,
        ask.scope.project_id,
        ask.from,
        ask.to,
    )
    .fetch_all(pool)
    .await?;
    let (users, active) =
        rows.iter().find(|row| row.start.is_none()).map_or((0, 0), |row| (row.users, row.active));
    let points = range
        .starts()
        .into_iter()
        .map(|start| {
            let row = rows.iter().find(|row| row.start == Some(start.0));
            InsightPoint { start, value: row.map_or(0.0, |row| ratio(row.users, row.active)) }
        })
        .collect();
    Ok(InsightResult::Series {
        measure: Measure::Share,
        interval: if range.hourly() { Interval::Hour } else { Interval::Day },
        value: ratio(users, active),
        active: Some(active),
        points,
        raw_since: range.raw_since(),
    })
}

/// People with a stored ID by how many times they did it.
async fn distribution(
    pool: &SqlitePool,
    ask: &Ask<'_>,
    range: Range,
) -> sqlx::Result<InsightResult> {
    let rows = sqlx::query!(
        r#"SELECT MIN(n, ?) AS "times!: i64", COUNT(*) AS "users!: i64"
           FROM (SELECT COUNT(*) AS n FROM analytics_events e
             WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ? AND name = ?
               AND substr(person, 1, 2) <> 'v:'
               AND NOT EXISTS (SELECT 1 FROM json_each(?) f WHERE NOT (
                 CASE json_extract(f.value, '$.op')
                 WHEN 'is_not' THEN NOT EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                   WHERE x.key = json_extract(f.value, '$.k')
                     AND (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END))
                 ELSE EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                   WHERE x.key = json_extract(f.value, '$.k') AND CASE json_extract(f.value, '$.op')
                     WHEN 'is' THEN (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END)
                     WHEN 'gt' THEN x.type IN ('integer', 'real') AND x.atom > json_extract(f.value, '$.n')
                     WHEN 'lt' THEN x.type IN ('integer', 'real') AND x.atom < json_extract(f.value, '$.n')
                     ELSE x.type <> 'null' END)
                 END))
             GROUP BY person)
           GROUP BY 1"#,
        TIMES_CAP,
        ask.scope.workspace_id,
        ask.scope.project_id,
        ask.from,
        ask.to,
        ask.event,
        ask.filters,
    )
    .fetch_all(pool)
    .await?;
    let buckets = (1..=TIMES_CAP)
        .map(|times| TimesBucket {
            times,
            or_more: times == TIMES_CAP,
            users: rows.iter().find(|row| row.times == times).map_or(0, |row| row.users),
        })
        .collect();
    Ok(InsightResult::Distribution {
        users: rows.iter().map(|row| row.users).sum(),
        buckets,
        raw_since: range.raw_since(),
    })
}

/// A breakdown query's row: a top value (none is "not set"), or the rest.
struct Grouped {
    in_top: bool,
    value: Option<String>,
    count: i64,
    users: i64,
}

/// The top values and the rest, in the measure's order.
fn split_rows(
    grouped: Vec<Grouped>,
    result: impl Fn(&Grouped) -> f64,
) -> (Vec<InsightRow>, Option<InsightRow>) {
    let mut rows = Vec::new();
    let mut other = None;
    for group in grouped {
        let row = InsightRow { result: result(&group), value: group.value };
        if group.in_top { rows.push(row) } else { other = Some(row) }
    }
    // Largest first; on a tie, values by name and "not set" last.
    rows.sort_by(|a, b| {
        b.result
            .total_cmp(&a.result)
            .then_with(|| a.value.is_none().cmp(&b.value.is_none()))
            .then_with(|| a.value.cmp(&b.value))
    });
    (rows, other)
}

/// Events split by a property or the person's trait.
async fn events_breakdown(
    pool: &SqlitePool,
    ask: &Ask<'_>,
    range: Range,
    measure: Measure,
    by: &BreakdownBy,
) -> sqlx::Result<InsightResult> {
    let share = measure == Measure::Share;
    let grouped = breakdown_groups(pool, ask, measure, by).await?;
    let (value, active) = if share {
        let (users, active) = share_totals(pool, ask).await?;
        (ratio(users, active), Some(active))
    } else {
        let (total, users) = totals(pool, ask).await?;
        (as_f64(if measure == Measure::Total { total } else { users }), None)
    };
    let (rows, other) = split_rows(grouped, |group| match measure {
        Measure::Total => as_f64(group.count),
        Measure::Share => ratio(group.users, active.unwrap_or(0)),
        Measure::Users | Measure::PerUser => as_f64(group.users),
    });
    Ok(InsightResult::Breakdown {
        measure: Some(measure),
        value,
        active,
        rows,
        other,
        raw_since: range.raw_since(),
    })
}

async fn breakdown_groups(
    pool: &SqlitePool,
    ask: &Ask<'_>,
    measure: Measure,
    by: &BreakdownBy,
) -> sqlx::Result<Vec<Grouped>> {
    let by_trait = by.source == BreakdownSource::Trait;
    let by_events = measure == Measure::Total;
    let stored_ids_only = measure == Measure::Share;
    let rows = sqlx::query!(
        r#"WITH m AS (
             SELECT CASE WHEN ? THEN
                 (SELECT CASE t.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                         ELSE CAST(t.atom AS TEXT) END
                  FROM analytics_people pp, json_each(pp.traits) t
                  WHERE substr(e.person, 1, 2) = 'p:' AND pp.id = substr(e.person, 3)
                    AND pp.workspace_id = e.workspace_id AND pp.project_id = e.project_id
                    AND t.key = ? AND t.type <> 'null')
               ELSE
                 (SELECT CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                         ELSE CAST(x.atom AS TEXT) END
                  FROM json_each(COALESCE(e.props, '{}')) x WHERE x.key = ? AND x.type <> 'null')
               END AS value, e.person AS person
             FROM analytics_events e
             WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ? AND name = ?
               AND (NOT ? OR substr(person, 1, 2) <> 'v:')
               AND NOT EXISTS (SELECT 1 FROM json_each(?) f WHERE NOT (
                 CASE json_extract(f.value, '$.op')
                 WHEN 'is_not' THEN NOT EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                   WHERE x.key = json_extract(f.value, '$.k')
                     AND (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END))
                 ELSE EXISTS (SELECT 1 FROM json_each(COALESCE(e.props, '{}')) x
                   WHERE x.key = json_extract(f.value, '$.k') AND CASE json_extract(f.value, '$.op')
                     WHEN 'is' THEN (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END)
                     WHEN 'gt' THEN x.type IN ('integer', 'real') AND x.atom > json_extract(f.value, '$.n')
                     WHEN 'lt' THEN x.type IN ('integer', 'real') AND x.atom < json_extract(f.value, '$.n')
                     ELSE x.type <> 'null' END)
                 END))
           ),
           top AS (
             SELECT value FROM m GROUP BY value
             ORDER BY CASE WHEN ? THEN COUNT(*) ELSE COUNT(DISTINCT person) END DESC, value IS NULL, value
             LIMIT ?
           ),
           t AS (SELECT value, person, EXISTS (SELECT 1 FROM top WHERE top.value IS m.value) AS in_top FROM m)
           SELECT in_top AS "in_top!: bool", CASE WHEN in_top THEN value END AS "value?: String",
                  COUNT(*) AS "count!: i64", COUNT(DISTINCT person) AS "users!: i64"
           FROM t GROUP BY 1, 2"#,
        by_trait,
        by.key,
        by.key,
        ask.scope.workspace_id,
        ask.scope.project_id,
        ask.from,
        ask.to,
        ask.event,
        stored_ids_only,
        ask.filters,
        by_events,
        TOP_VALUES,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Grouped {
            in_top: row.in_top,
            value: row.value,
            count: row.count,
            users: row.users,
        })
        .collect())
}

/// People matching the trait filters, and everyone known.
async fn people_count(pool: &SqlitePool, ask: &Ask<'_>) -> sqlx::Result<InsightResult> {
    let row = sqlx::query!(
        r#"SELECT COUNT(*) AS "known!: i64",
                  COALESCE(SUM(NOT EXISTS (SELECT 1 FROM json_each(?) f WHERE NOT (
                    CASE json_extract(f.value, '$.op')
                    WHEN 'is_not' THEN NOT EXISTS (SELECT 1 FROM json_each(p.traits) x
                      WHERE x.key = json_extract(f.value, '$.k')
                        AND (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END))
                    ELSE EXISTS (SELECT 1 FROM json_each(p.traits) x
                      WHERE x.key = json_extract(f.value, '$.k') AND CASE json_extract(f.value, '$.op')
                        WHEN 'is' THEN (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END)
                        WHEN 'gt' THEN x.type IN ('integer', 'real') AND x.atom > json_extract(f.value, '$.n')
                        WHEN 'lt' THEN x.type IN ('integer', 'real') AND x.atom < json_extract(f.value, '$.n')
                        ELSE x.type <> 'null' END)
                    END))), 0) AS "people!: i64"
           FROM analytics_people p
           WHERE workspace_id = ? AND project_id = ?"#,
        ask.filters,
        ask.scope.workspace_id,
        ask.scope.project_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(InsightResult::Count { people: row.people, known: row.known })
}

/// Matching people split by a trait.
async fn people_breakdown(
    pool: &SqlitePool,
    ask: &Ask<'_>,
    by: &BreakdownBy,
) -> sqlx::Result<InsightResult> {
    let found = sqlx::query!(
        r#"WITH m AS (
             SELECT (SELECT CASE t.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                             ELSE CAST(t.atom AS TEXT) END
                      FROM json_each(p.traits) t WHERE t.key = ? AND t.type <> 'null') AS value
             FROM analytics_people p
             WHERE workspace_id = ? AND project_id = ?
               AND NOT EXISTS (SELECT 1 FROM json_each(?) f WHERE NOT (
                 CASE json_extract(f.value, '$.op')
                 WHEN 'is_not' THEN NOT EXISTS (SELECT 1 FROM json_each(p.traits) x
                   WHERE x.key = json_extract(f.value, '$.k')
                     AND (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END))
                 ELSE EXISTS (SELECT 1 FROM json_each(p.traits) x
                   WHERE x.key = json_extract(f.value, '$.k') AND CASE json_extract(f.value, '$.op')
                     WHEN 'is' THEN (CASE WHEN x.type IN ('integer', 'real') AND json_extract(f.value, '$.n') IS NOT NULL
                         THEN x.atom = json_extract(f.value, '$.n')
                         ELSE (CASE x.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                               ELSE CAST(x.atom AS TEXT) END) = json_extract(f.value, '$.v') END)
                     WHEN 'gt' THEN x.type IN ('integer', 'real') AND x.atom > json_extract(f.value, '$.n')
                     WHEN 'lt' THEN x.type IN ('integer', 'real') AND x.atom < json_extract(f.value, '$.n')
                     ELSE x.type <> 'null' END)
                 END))
           ),
           top AS (SELECT value FROM m GROUP BY value ORDER BY COUNT(*) DESC, value IS NULL, value LIMIT ?),
           t AS (SELECT value, EXISTS (SELECT 1 FROM top WHERE top.value IS m.value) AS in_top FROM m)
           SELECT in_top AS "in_top!: bool", CASE WHEN in_top THEN value END AS "value?: String",
                  COUNT(*) AS "count!: i64"
           FROM t GROUP BY 1, 2"#,
        by.key,
        ask.scope.workspace_id,
        ask.scope.project_id,
        ask.filters,
        TOP_VALUES,
    )
    .fetch_all(pool)
    .await?;
    let grouped: Vec<Grouped> = found
        .into_iter()
        .map(|row| Grouped {
            in_top: row.in_top,
            value: row.value,
            count: row.count,
            users: row.count,
        })
        .collect();
    let matched = grouped.iter().map(|group| group.count).sum();
    let known = known_people(pool, ask.scope).await?;
    let (rows, other) = split_rows(grouped, |group| as_f64(group.count));
    Ok(InsightResult::Breakdown {
        measure: None,
        value: as_f64(matched),
        active: Some(known),
        rows,
        other,
        raw_since: None,
    })
}

async fn known_people(pool: &SqlitePool, scope: Scope) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM analytics_people WHERE workspace_id = ? AND project_id = ?"#,
        scope.workspace_id,
        scope.project_id,
    )
    .fetch_one(pool)
    .await
}
