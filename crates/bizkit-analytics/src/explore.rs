//! The event explorer: `events.query` counts matching events over time and
//! lists the names seen; `events.recent` pages through the events. Both read
//! raw events, so they cover the raw retention window.
//!
//! Property filters travel to SQLite as one JSON array, matched with
//! `json_each`, so any number of them stays one static query.

use bizkit_core::ApiError;
use bizkit_core::ModuleContext;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Level, Method, OpCtx, Operation, Scope as AccessScope};
use bizkit_core::time::UnixMillis;
use bizkit_core::validate::{FieldErrors, Validate};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::SqliteExecutor;
use utoipa::ToSchema;

use crate::api::{Day, Interval, PersonId};
use crate::names;
use crate::report::Scope;
use crate::stats::{check_range, site_scope};

const READ: Access = Access::Workspace(AccessScope::new("analytics", Level::Read));
const TAGS: &[&str] = &["analytics"];
const HOUR_MS: i64 = 3_600_000;
const DEFAULT_PAGE: u32 = 50;
const MAX_PAGE: u32 = 200;
const MAX_FILTERS: usize = 10;

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.op(EventsQuery)?;
    cx.op(EventsRecent)
}

/// The SQL side of a filter: the name (`None` is every custom event) and
/// the props as `[{"k": .., "v": ..}]`.
struct Filter {
    name: Option<String>,
    props: String,
}

fn filter(name: Option<&str>, props: &[String]) -> Filter {
    let pairs: Vec<Value> = props
        .iter()
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| json!({"k": key, "v": value}))
        .collect();
    Filter { name: name.map(str::to_owned), props: Value::Array(pairs).to_string() }
}

fn check_filters(errors: &mut FieldErrors, name: Option<&str>, props: &[String]) {
    if name.is_some_and(|name| name != crate::beacon::PAGEVIEW && !names::is_event_name(name)) {
        errors.add("name", names::EVENT_NAME_RULE);
    }
    if props.len() > MAX_FILTERS {
        errors.add("props", format!("must have at most {MAX_FILTERS} filters"));
    }
    if props.iter().any(|pair| pair.split_once('=').is_none_or(|(key, _)| key.is_empty())) {
        errors.add("props", "each filter must be key=value");
    }
}

/// Event counts over a range.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryEvents {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The first day, UTC.
    from: Day,
    /// The last day, UTC, inclusive.
    to: Day,
    /// Only this event name (`pageview` included); default every custom event.
    #[serde(default)]
    name: Option<String>,
    /// Property filters, `key=value`; all must match.
    #[serde(default)]
    props: Vec<String>,
}

impl Validate for QueryEvents {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_range(&mut errors, self.from, self.to);
        check_filters(&mut errors, self.name.as_deref(), &self.props);
        errors.into_result()
    }
}

/// Matching events per hour or day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct EventPoint {
    /// When the hour or day starts.
    pub(crate) start: UnixMillis,
    /// Matching events.
    pub(crate) count: i64,
    /// Distinct people (or daily visitors) who sent them.
    pub(crate) users: i64,
}

/// One event name seen in the range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct EventName {
    /// The name.
    pub(crate) name: String,
    /// Matching events with this name.
    pub(crate) count: i64,
    /// Distinct people (or daily visitors) who sent them.
    pub(crate) users: i64,
}

/// The explorer's chart and name table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct EventStats {
    /// What one point covers.
    pub(crate) interval: Interval,
    /// Counts over time.
    pub(crate) series: Vec<EventPoint>,
    /// Custom event names in the range with the props filter applied, most
    /// sent first. Ignores the name filter, so it can fill a picker.
    pub(crate) names: Vec<EventName>,
}

struct EventsQuery;
impl Operation for EventsQuery {
    type Input = QueryEvents;
    type Output = EventStats;
    const ID: &'static str = "analytics.events.query";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/events/stats";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Count product events over time, filtered by name and properties";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: QueryEvents) -> Result<EventStats, ApiError> {
        let db = cx.platform().db();
        let (scope, _) = site_scope(db, input.workspace_id, input.project_id).await?;
        let filter = filter(input.name.as_deref(), &input.props);
        let hourly = input.from == input.to;
        let buckets = buckets(db.read(), scope, (input.from, input.to), &filter, hourly).await?;
        let series = if hourly {
            (0..24)
                .map(|hour| {
                    let start = UnixMillis(input.from.start().0 + hour * HOUR_MS);
                    point(&buckets, start)
                })
                .collect()
        } else {
            let mut points = Vec::new();
            let mut day = input.from;
            while day <= input.to {
                points.push(point(&buckets, day.start()));
                day = day.next();
            }
            points
        };
        let names = event_names(db.read(), scope, input.from, input.to, &filter.props).await?;
        let interval = if hourly { Interval::Hour } else { Interval::Day };
        Ok(EventStats { interval, series, names })
    }
}

fn point(buckets: &[EventPoint], start: UnixMillis) -> EventPoint {
    buckets.iter().find(|b| b.start == start).copied().unwrap_or(EventPoint {
        start,
        count: 0,
        users: 0,
    })
}

async fn buckets(
    exec: impl SqliteExecutor<'_>,
    scope: Scope,
    (from, to): (Day, Day),
    filter: &Filter,
    hourly: bool,
) -> sqlx::Result<Vec<EventPoint>> {
    let (from_text, to_text) = (from.to_string(), to.to_string());
    let bucket_ms = if hourly { HOUR_MS } else { 86_400_000 };
    let origin = from.start();
    let rows = sqlx::query!(
        r#"SELECT ? + ((ts - ?) / ?) * ? AS "start!: i64", COUNT(*) AS "count!: i64",
                  COUNT(DISTINCT person) AS "users!: i64"
           FROM analytics_events e
           WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ?
             AND (CASE WHEN ? IS NULL THEN name <> 'pageview' ELSE name = ? END)
             AND NOT EXISTS (
               SELECT 1 FROM json_each(?) f
               WHERE NOT EXISTS (
                 SELECT 1 FROM json_each(COALESCE(e.props, '{}')) p
                 WHERE p.key = json_extract(f.value, '$.k')
                   AND CASE p.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                                   ELSE CAST(p.atom AS TEXT) END = json_extract(f.value, '$.v')))
           GROUP BY 1"#,
        origin,
        origin,
        bucket_ms,
        bucket_ms,
        scope.workspace_id,
        scope.project_id,
        from_text,
        to_text,
        filter.name,
        filter.name,
        filter.props,
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| EventPoint { start: UnixMillis(row.start), count: row.count, users: row.users })
        .collect())
}

async fn event_names(
    exec: impl SqliteExecutor<'_>,
    scope: Scope,
    from: Day,
    to: Day,
    props: &str,
) -> sqlx::Result<Vec<EventName>> {
    let (from, to) = (from.to_string(), to.to_string());
    let rows = sqlx::query!(
        r#"SELECT name, COUNT(*) AS "count!: i64", COUNT(DISTINCT person) AS "users!: i64"
           FROM analytics_events e
           WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ? AND name <> 'pageview'
             AND NOT EXISTS (
               SELECT 1 FROM json_each(?) f
               WHERE NOT EXISTS (
                 SELECT 1 FROM json_each(COALESCE(e.props, '{}')) p
                 WHERE p.key = json_extract(f.value, '$.k')
                   AND CASE p.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                                   ELSE CAST(p.atom AS TEXT) END = json_extract(f.value, '$.v')))
           GROUP BY name ORDER BY 2 DESC, name LIMIT 200"#,
        scope.workspace_id,
        scope.project_id,
        from,
        to,
        props,
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| EventName { name: row.name, count: row.count, users: row.users })
        .collect())
}

/// A page of recent events.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecentEvents {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The first day, UTC.
    from: Day,
    /// The last day, UTC, inclusive.
    to: Day,
    /// Only this event name; default every event, pageviews included.
    #[serde(default)]
    name: Option<String>,
    /// Property filters, `key=value`; all must match.
    #[serde(default)]
    props: Vec<String>,
    /// Events older than this one, from `next_before`.
    #[serde(default)]
    before: Option<i64>,
    /// At most this many, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}

impl Validate for RecentEvents {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_range(&mut errors, self.from, self.to);
        check_filters(&mut errors, self.name.as_deref(), &self.props);
        if self.limit.is_some_and(|limit| !(1..=MAX_PAGE).contains(&limit)) {
            errors.add("limit", format!("must be between 1 and {MAX_PAGE}"));
        }
        errors.into_result()
    }
}

/// One stored event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct EventItem {
    /// The event's row ID; pass it as `before` for the next page.
    pub(crate) id: i64,
    /// When it arrived.
    pub(crate) ts: UnixMillis,
    /// The event name.
    pub(crate) name: String,
    /// The page path, when it came with a URL.
    pub(crate) path: Option<String>,
    /// The identified person, if any.
    pub(crate) person_id: Option<PersonId>,
    /// The person's user ID, if identified.
    pub(crate) user_id: Option<String>,
    /// Event properties.
    #[schema(value_type = Object)]
    pub(crate) props: Map<String, Value>,
    /// `web` (the snippet) or `server`.
    pub(crate) source: String,
}

/// A page of events, newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct EventPage {
    /// The events.
    pub(crate) items: Vec<EventItem>,
    /// Pass as `before` for the next page; none on the last page.
    pub(crate) next_before: Option<i64>,
}

struct EventsRecent;
impl Operation for EventsRecent {
    type Input = RecentEvents;
    type Output = EventPage;
    const ID: &'static str = "analytics.events.recent";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/events";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List recent analytics events, newest first";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: RecentEvents) -> Result<EventPage, ApiError> {
        let db = cx.platform().db();
        let (scope, _) = site_scope(db, input.workspace_id, input.project_id).await?;
        let filter = filter(input.name.as_deref(), &input.props);
        let limit = input.limit.unwrap_or(DEFAULT_PAGE);
        let (from, to) = (input.from.to_string(), input.to.to_string());
        let before = input.before.unwrap_or(i64::MAX);
        let fetch = i64::from(limit) + 1;
        let rows = sqlx::query!(
            r#"SELECT e.id AS "id!: i64", e.ts AS "ts: UnixMillis", e.name, e.path, e.props, e.source,
                      p.id AS "person_id?: PersonId", p.user_id AS "user_id?"
               FROM analytics_events e
               LEFT JOIN analytics_people p ON p.id = substr(e.person, 3) AND substr(e.person, 1, 2) = 'p:'
               WHERE e.workspace_id = ? AND e.project_id = ? AND e.day >= ? AND e.day <= ? AND e.id < ?
                 AND (? IS NULL OR e.name = ?)
                 AND NOT EXISTS (
                   SELECT 1 FROM json_each(?) f
                   WHERE NOT EXISTS (
                     SELECT 1 FROM json_each(COALESCE(e.props, '{}')) pr
                     WHERE pr.key = json_extract(f.value, '$.k')
                       AND CASE pr.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                                        ELSE CAST(pr.atom AS TEXT) END = json_extract(f.value, '$.v')))
               ORDER BY e.id DESC LIMIT ?"#,
            scope.workspace_id,
            scope.project_id,
            from,
            to,
            before,
            filter.name,
            filter.name,
            filter.props,
            fetch,
        )
        .fetch_all(db.read())
        .await?;
        let mut items: Vec<EventItem> = rows
            .into_iter()
            .map(|row| EventItem {
                id: row.id,
                ts: row.ts,
                name: row.name,
                path: row.path,
                person_id: row.person_id,
                user_id: row.user_id,
                props: row
                    .props
                    .and_then(|text| serde_json::from_str(&text).ok())
                    .unwrap_or_default(),
                source: row.source,
            })
            .collect();
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let more = items.len() > limit;
        items.truncate(limit);
        let next_before = if more { items.last().map(|item| item.id) } else { None };
        Ok(EventPage { items, next_before })
    }
}
