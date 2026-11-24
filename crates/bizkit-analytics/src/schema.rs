//! What a project has sent, for pickers and for agents writing insight
//! queries: event names, property and trait keys, and their top values.
//!
//! Insight filters and breakdowns read only `analytics_events.props` and
//! `analytics_people.traits`, so those are the two sources. Each lookup
//! scans at most [`SCAN_CAP`] rows, newest first, and says so with
//! `sampled`, so a large project file still answers while someone types.
//! Keys arrive as bound parameters and match `json_each`'s `key` column,
//! never a JSON path, so no key can change the SQL or the path syntax.

use std::sync::Arc;

use bizkit_core::ApiError;
use bizkit_core::ModuleContext;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Level, Method, OpCtx, Operation, Scope as AccessScope};
use bizkit_core::time::UnixMillis;
use bizkit_core::validate::{FieldErrors, Validate};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::Day;
use crate::beacon::MAX_PROP_KEY;
use crate::files::Files;
use crate::names;
use crate::people::like_pattern;
use crate::stats::{check_range, site_scope};

const READ: Access = Access::Workspace(AccessScope::new("analytics", Level::Read));
const TAGS: &[&str] = &["analytics"];
/// Rows one lookup reads at most, newest first.
pub(crate) const SCAN_CAP: i64 = 50_000;
/// Days a lookup covers when the input gives no range.
const DEFAULT_DAYS: i64 = 30;
const MAX_LIMIT: u32 = 200;
const MAX_Q: usize = 128;

pub(crate) fn register(cx: &mut ModuleContext<'_>, files: &Arc<Files>) -> anyhow::Result<()> {
    cx.op(SchemaEvents(Arc::clone(files)))?;
    cx.op(SchemaKeys(Arc::clone(files)))?;
    cx.op(SchemaValues(Arc::clone(files)))
}

/// Where keys and values come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SchemaSource {
    /// Event properties, from raw events in the range.
    Property,
    /// Person traits, from every known person; the range does not apply.
    Trait,
}

/// The JSON type of a key's values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SchemaValueType {
    Text,
    Number,
    Boolean,
}

impl SchemaValueType {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "text" => Some(Self::Text),
            "number" => Some(Self::Number),
            "boolean" => Some(Self::Boolean),
            _ => None,
        }
    }
}

fn check_common(
    errors: &mut FieldErrors,
    range: (Option<Day>, Option<Day>),
    q: Option<&str>,
    limit: Option<u32>,
) {
    match range {
        (Some(from), Some(to)) => check_range(errors, from, to),
        (None, None) => {}
        _ => errors.add("to", "from and to must be given together"),
    }
    if q.is_some_and(|q| q.chars().count() > MAX_Q) {
        errors.add("q", format!("must be at most {MAX_Q} characters"));
    }
    if limit.is_some_and(|limit| !(1..=MAX_LIMIT).contains(&limit)) {
        errors.add("limit", format!("must be between 1 and {MAX_LIMIT}"));
    }
}

fn check_event(errors: &mut FieldErrors, source: SchemaSource, event: Option<&str>) {
    let Some(event) = event else { return };
    if source == SchemaSource::Trait {
        errors.add("event", "applies only to the property source");
    } else if event != crate::beacon::PAGEVIEW && !names::is_event_name(event) {
        errors.add("event", names::EVENT_NAME_RULE);
    }
}

/// The scan window in unix milliseconds, start inclusive and end
/// exclusive; the last [`DEFAULT_DAYS`] UTC days when no range is given.
fn window(now: UnixMillis, from: Option<Day>, to: Option<Day>) -> (i64, i64) {
    let today = Day::of(now);
    let to = to.unwrap_or(today);
    let from = from.unwrap_or_else(|| today.plus_days(1 - DEFAULT_DAYS));
    (from.start().0, to.next().start().0)
}

fn pattern(q: Option<&str>) -> Option<String> {
    q.filter(|q| !q.trim().is_empty()).map(like_pattern)
}

/// Which event names to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListSchemaEvents {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The first day, UTC. Give it with `to`; default 29 days before today.
    #[serde(default)]
    from: Option<Day>,
    /// The last day, UTC, inclusive. Give it with `from`; default today.
    #[serde(default)]
    to: Option<Day>,
    /// Text the name contains, ignoring ASCII case.
    #[serde(default)]
    q: Option<String>,
    /// At most this many, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}

impl Validate for ListSchemaEvents {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_common(&mut errors, (self.from, self.to), self.q.as_deref(), self.limit);
        errors.into_result()
    }
}

/// One event name seen in the scanned events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct SchemaEvent {
    /// The name; `pageview` for page views.
    pub(crate) name: String,
    /// Scanned events with this name.
    pub(crate) events: i64,
    /// Distinct people (or daily visitors) who sent them.
    pub(crate) users: i64,
    /// When the newest one arrived.
    pub(crate) last_seen: UnixMillis,
}

/// Event names, most sent first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct SchemaEventList {
    /// The names.
    pub(crate) items: Vec<SchemaEvent>,
    /// The scan stopped at its cap of newest events, so older events in the
    /// range are not counted and rare names can be missing.
    pub(crate) sampled: bool,
}

struct SchemaEvents(Arc<Files>);
impl Operation for SchemaEvents {
    type Input = ListSchemaEvents;
    type Output = SchemaEventList;
    const ID: &'static str = "analytics.schema.events";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/schema/events";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str =
        "List the event names a project sent, pageview included, most sent first";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListSchemaEvents) -> Result<SchemaEventList, ApiError> {
        let (scope, _) =
            site_scope(cx.platform().db(), input.workspace_id, input.project_id).await?;
        let file = self.0.for_op(cx.platform(), scope).await?;
        let (start, end) = window(cx.now(), input.from, input.to);
        let pattern = pattern(input.q.as_deref());
        let limit = i64::from(input.limit.unwrap_or(50));
        let rows = sqlx::query!(
            r#"WITH s AS MATERIALIZED (
                 SELECT name, person, ts FROM analytics_events
                 WHERE workspace_id = ? AND project_id = ? AND ts >= ? AND ts < ?
                   AND (? IS NULL OR name LIKE ? ESCAPE '\')
                 ORDER BY ts DESC LIMIT ?),
               agg AS (
                 SELECT name, COUNT(*) AS events, COUNT(DISTINCT person) AS users, MAX(ts) AS last_seen
                 FROM s GROUP BY name ORDER BY 2 DESC, name LIMIT ?)
               SELECT n.scanned AS "scanned!: i64", agg.name AS "name?: String",
                      agg.events AS "events?: i64", agg.users AS "users?: i64",
                      agg.last_seen AS "last_seen?: i64"
               FROM (SELECT COUNT(*) AS scanned FROM s) n LEFT JOIN agg
               ORDER BY agg.events DESC, agg.name"#,
            scope.workspace_id,
            scope.project_id,
            start,
            end,
            pattern,
            pattern,
            SCAN_CAP,
            limit,
        )
        .fetch_all(file.db.read())
        .await?;
        let sampled = rows.first().is_some_and(|row| row.scanned >= SCAN_CAP);
        let items = rows
            .into_iter()
            .filter_map(|row| {
                Some(SchemaEvent {
                    name: row.name?,
                    events: row.events?,
                    users: row.users?,
                    last_seen: UnixMillis(row.last_seen?),
                })
            })
            .collect();
        Ok(SchemaEventList { items, sampled })
    }
}

/// Which keys to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListSchemaKeys {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// Event properties or person traits.
    source: SchemaSource,
    /// Only properties of this event (`pageview` included). Property source only.
    #[serde(default)]
    event: Option<String>,
    /// The first day, UTC. Give it with `to`; default 29 days before today.
    /// The trait source ignores the range.
    #[serde(default)]
    from: Option<Day>,
    /// The last day, UTC, inclusive. Give it with `from`; default today.
    #[serde(default)]
    to: Option<Day>,
    /// Text the key contains, ignoring ASCII case.
    #[serde(default)]
    q: Option<String>,
    /// At most this many, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}

impl Validate for ListSchemaKeys {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_common(&mut errors, (self.from, self.to), self.q.as_deref(), self.limit);
        check_event(&mut errors, self.source, self.event.as_deref());
        errors.into_result()
    }
}

/// One property or trait key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct SchemaKey {
    /// The key.
    pub(crate) key: String,
    /// The types its values take, in this order: text, number, boolean.
    pub(crate) types: Vec<SchemaValueType>,
    /// Scanned events (or people) that have it with a text, number or
    /// boolean value.
    pub(crate) count: i64,
}

/// Keys, most used first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct SchemaKeyList {
    /// The keys.
    pub(crate) items: Vec<SchemaKey>,
    /// The scan stopped at its cap of newest events (or most recently seen
    /// people), so rare keys can be missing.
    pub(crate) sampled: bool,
}

struct SchemaKeys(Arc<Files>);
impl Operation for SchemaKeys {
    type Input = ListSchemaKeys;
    type Output = SchemaKeyList;
    const ID: &'static str = "analytics.schema.keys";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/schema/keys";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str =
        "List the event property or person trait keys a project has, with their value types";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListSchemaKeys) -> Result<SchemaKeyList, ApiError> {
        let (scope, _) =
            site_scope(cx.platform().db(), input.workspace_id, input.project_id).await?;
        let file = self.0.for_op(cx.platform(), scope).await?;
        let (start, end) = window(cx.now(), input.from, input.to);
        let pattern = pattern(input.q.as_deref());
        let limit = i64::from(input.limit.unwrap_or(50));
        let is_trait = input.source == SchemaSource::Trait;
        // One static statement for both sources: a flag parameter empties
        // the CTE of the source not asked for, so sqlx checks all of it.
        // The CAST on `j.key` keeps sqlx's nullability check from looking
        // up `json_each.key` as a stored column, which it can't find.
        let rows = sqlx::query!(
            r#"WITH s AS MATERIALIZED (
                 SELECT props AS doc FROM analytics_events
                 WHERE NOT ? AND workspace_id = ? AND project_id = ? AND ts >= ? AND ts < ?
                   AND (? IS NULL OR name = ?) AND props IS NOT NULL AND props <> '{}'
                 ORDER BY ts DESC LIMIT ?),
               t AS MATERIALIZED (
                 SELECT traits AS doc FROM analytics_people
                 WHERE ? AND workspace_id = ? AND project_id = ? AND traits <> '{}'
                 ORDER BY last_seen DESC LIMIT ?),
               d AS (SELECT doc FROM s UNION ALL SELECT doc FROM t),
               k AS (
                 SELECT CAST(j.key AS TEXT) AS key,
                        CASE j.type WHEN 'text' THEN 'text' WHEN 'integer' THEN 'number'
                                    WHEN 'real' THEN 'number' WHEN 'true' THEN 'boolean'
                                    WHEN 'false' THEN 'boolean' END AS kind
                 FROM d, json_each(d.doc) j),
               agg AS (
                 SELECT key, COUNT(*) AS count, GROUP_CONCAT(DISTINCT kind) AS kinds FROM k
                 WHERE kind IS NOT NULL AND (? IS NULL OR key LIKE ? ESCAPE '\')
                 GROUP BY key ORDER BY 2 DESC, key LIMIT ?)
               SELECT n.scanned AS "scanned!: i64", agg.key AS "key?: String",
                      agg.count AS "count?: i64", agg.kinds AS "kinds?: String"
               FROM (SELECT (SELECT COUNT(*) FROM s) + (SELECT COUNT(*) FROM t) AS scanned) n
               LEFT JOIN agg
               ORDER BY agg.count DESC, agg.key"#,
            is_trait,
            scope.workspace_id,
            scope.project_id,
            start,
            end,
            input.event,
            input.event,
            SCAN_CAP,
            is_trait,
            scope.workspace_id,
            scope.project_id,
            SCAN_CAP,
            pattern,
            pattern,
            limit,
        )
        .fetch_all(file.db.read())
        .await?;
        let sampled = rows.first().is_some_and(|row| row.scanned >= SCAN_CAP);
        let items = rows
            .into_iter()
            .filter_map(|row| {
                let mut types: Vec<SchemaValueType> =
                    row.kinds?.split(',').filter_map(SchemaValueType::parse).collect();
                types.sort_unstable();
                types.dedup();
                Some(SchemaKey { key: row.key?, types, count: row.count? })
            })
            .collect();
        Ok(SchemaKeyList { items, sampled })
    }
}

/// Which values to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListSchemaValues {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// Event properties or person traits.
    source: SchemaSource,
    /// The property or trait key, matched exactly.
    key: String,
    /// Only values on this event (`pageview` included). Property source only.
    #[serde(default)]
    event: Option<String>,
    /// The first day, UTC. Give it with `to`; default 29 days before today.
    /// The trait source ignores the range.
    #[serde(default)]
    from: Option<Day>,
    /// The last day, UTC, inclusive. Give it with `from`; default today.
    #[serde(default)]
    to: Option<Day>,
    /// Text the value contains, ignoring ASCII case.
    #[serde(default)]
    q: Option<String>,
    /// At most this many, 1-200. Default 20.
    #[serde(default)]
    limit: Option<u32>,
}

impl Validate for ListSchemaValues {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_common(&mut errors, (self.from, self.to), self.q.as_deref(), self.limit);
        check_event(&mut errors, self.source, self.event.as_deref());
        if self.key.is_empty() || self.key.chars().count() > MAX_PROP_KEY {
            errors.add("key", format!("must be 1-{MAX_PROP_KEY} characters"));
        }
        errors.into_result()
    }
}

/// One value a key takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct SchemaValue {
    /// The value in text form, as insight filters compare it: numbers as
    /// written, booleans as `true` or `false`.
    pub(crate) value: String,
    /// Scanned events (or people) with this value.
    pub(crate) count: i64,
}

/// Top values, most common first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct SchemaValueList {
    /// The values.
    pub(crate) items: Vec<SchemaValue>,
    /// The scan stopped at its cap of newest events (or most recently seen
    /// people) with the key, so counts cover only those.
    pub(crate) sampled: bool,
}

struct SchemaValues(Arc<Files>);
impl Operation for SchemaValues {
    type Input = ListSchemaValues;
    type Output = SchemaValueList;
    const ID: &'static str = "analytics.schema.values";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/schema/values";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str =
        "List the top values of an event property or person trait, in text form";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListSchemaValues) -> Result<SchemaValueList, ApiError> {
        let (scope, _) =
            site_scope(cx.platform().db(), input.workspace_id, input.project_id).await?;
        let file = self.0.for_op(cx.platform(), scope).await?;
        let (start, end) = window(cx.now(), input.from, input.to);
        let pattern = pattern(input.q.as_deref());
        let limit = i64::from(input.limit.unwrap_or(20));
        let is_trait = input.source == SchemaSource::Trait;
        let rows = sqlx::query!(
            r#"WITH s AS MATERIALIZED (
                 SELECT props AS doc FROM analytics_events e
                 WHERE NOT ? AND workspace_id = ? AND project_id = ? AND ts >= ? AND ts < ?
                   AND (? IS NULL OR name = ?) AND props IS NOT NULL
                   AND EXISTS (SELECT 1 FROM json_each(e.props) x WHERE x.key = ?)
                 ORDER BY ts DESC LIMIT ?),
               t AS MATERIALIZED (
                 SELECT traits AS doc FROM analytics_people p
                 WHERE ? AND workspace_id = ? AND project_id = ?
                   AND EXISTS (SELECT 1 FROM json_each(p.traits) x WHERE x.key = ?)
                 ORDER BY last_seen DESC LIMIT ?),
               d AS (SELECT doc FROM s UNION ALL SELECT doc FROM t),
               v AS (
                 SELECT CASE j.type WHEN 'true' THEN 'true' WHEN 'false' THEN 'false'
                                    ELSE CAST(j.atom AS TEXT) END AS value
                 FROM d, json_each(d.doc) j
                 WHERE j.key = ? AND j.type IN ('text', 'integer', 'real', 'true', 'false')),
               agg AS (
                 SELECT value, COUNT(*) AS count FROM v
                 WHERE ? IS NULL OR value LIKE ? ESCAPE '\'
                 GROUP BY value ORDER BY 2 DESC, value LIMIT ?)
               SELECT n.scanned AS "scanned!: i64", agg.value AS "value?: String",
                      agg.count AS "count?: i64"
               FROM (SELECT (SELECT COUNT(*) FROM s) + (SELECT COUNT(*) FROM t) AS scanned) n
               LEFT JOIN agg
               ORDER BY agg.count DESC, agg.value"#,
            is_trait,
            scope.workspace_id,
            scope.project_id,
            start,
            end,
            input.event,
            input.event,
            input.key,
            SCAN_CAP,
            is_trait,
            scope.workspace_id,
            scope.project_id,
            input.key,
            SCAN_CAP,
            input.key,
            pattern,
            pattern,
            limit,
        )
        .fetch_all(file.db.read())
        .await?;
        let sampled = rows.first().is_some_and(|row| row.scanned >= SCAN_CAP);
        let items = rows
            .into_iter()
            .filter_map(|row| Some(SchemaValue { value: row.value?, count: row.count? }))
            .collect();
        Ok(SchemaValueList { items, sampled })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_defaults_to_the_last_30_days() {
        let now = "2026-03-15".parse::<Day>().unwrap().start();
        let (start, end) = window(UnixMillis(now.0 + 5_000), None, None);
        assert_eq!(start, "2026-02-14".parse::<Day>().unwrap().start().0);
        assert_eq!(end, "2026-03-16".parse::<Day>().unwrap().start().0);
    }

    #[test]
    fn range_must_come_whole() {
        let from = "2026-01-01".parse::<Day>().unwrap();
        let mut errors = FieldErrors::default();
        check_common(&mut errors, (Some(from), None), None, None);
        assert!(errors.into_result().is_err());
    }
}
