//! Identified people, their timelines, and weekly retention cohorts.

use std::collections::{BTreeMap, HashMap, HashSet};

use bizkit_core::ApiError;
use bizkit_core::ModuleContext;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Level, Method, OpCtx, Operation, Scope as AccessScope};
use bizkit_core::time::UnixMillis;
use bizkit_core::validate::{FieldErrors, Validate};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

use crate::api::{Day, PersonId};
use crate::explore::EventItem;
use crate::names;
use crate::stats::{check_range, site_scope};

const READ: Access = Access::Workspace(AccessScope::new("analytics", Level::Read));
const TAGS: &[&str] = &["analytics"];
const DEFAULT_PAGE: u32 = 50;
const MAX_PAGE: u32 = 200;
/// Weeks after the cohort week that retention reports.
pub(crate) const RETENTION_WEEKS: usize = 8;

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.op(PeopleList)?;
    cx.op(PeopleGet)?;
    cx.op(Retention)
}

fn check_limit(errors: &mut FieldErrors, limit: Option<u32>) {
    if limit.is_some_and(|limit| !(1..=MAX_PAGE).contains(&limit)) {
        errors.add("limit", format!("must be between 1 and {MAX_PAGE}"));
    }
}

/// A known person, as a list row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct PersonSummary {
    /// The person ID.
    pub(crate) id: PersonId,
    /// The site's ID for the user.
    pub(crate) user_id: String,
    /// The `email` or `name` trait, when there is one.
    pub(crate) display: Option<String>,
    /// When their first event or identify arrived.
    pub(crate) first_seen: UnixMillis,
    /// When their latest one arrived.
    pub(crate) last_seen: UnixMillis,
    /// Stored events under this person.
    pub(crate) events: i64,
}

/// A page of people, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct PersonPage {
    /// The people.
    pub(crate) items: Vec<PersonSummary>,
    /// Pass as `before` for the next page; none on the last page.
    pub(crate) next_before: Option<PersonId>,
}

/// Which people to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListPeople {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// Text to find in the user ID or traits.
    #[serde(default)]
    q: Option<String>,
    /// People older than this one, from `next_before`.
    #[serde(default)]
    before: Option<PersonId>,
    /// At most this many, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}

impl Validate for ListPeople {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_limit(&mut errors, self.limit);
        if self.q.as_ref().is_some_and(|q| q.chars().count() > 128) {
            errors.add("q", "must be at most 128 characters");
        }
        errors.into_result()
    }
}

fn display(traits: &Map<String, Value>) -> Option<String> {
    ["email", "name"].iter().find_map(|key| traits.get(*key)?.as_str().map(str::to_owned))
}

fn like_pattern(q: &str) -> String {
    let mut pattern = String::from("%");
    for c in q.trim().chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

struct PeopleList;
impl Operation for PeopleList {
    type Input = ListPeople;
    type Output = PersonPage;
    const ID: &'static str = "analytics.people.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/people";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List identified people, newest first";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListPeople) -> Result<PersonPage, ApiError> {
        let limit = input.limit.unwrap_or(DEFAULT_PAGE);
        let pattern = input.q.as_deref().filter(|q| !q.trim().is_empty()).map(like_pattern);
        let fetch = i64::from(limit) + 1;
        let rows = sqlx::query!(
            r#"SELECT id AS "id: PersonId", user_id, traits, first_seen AS "first_seen: UnixMillis",
                      last_seen AS "last_seen: UnixMillis",
                      (SELECT COUNT(*) FROM analytics_events e
                       WHERE e.project_id = p.project_id AND e.person = 'p:' || p.id) AS "events!: i64"
               FROM analytics_people p
               WHERE workspace_id = ? AND project_id = ? AND (? IS NULL OR id < ?)
                 AND (? IS NULL OR user_id LIKE ? ESCAPE '\' OR traits LIKE ? ESCAPE '\')
               ORDER BY id DESC LIMIT ?"#,
            input.workspace_id,
            input.project_id,
            input.before,
            input.before,
            pattern,
            pattern,
            pattern,
            fetch,
        )
        .fetch_all(cx.platform().db().read())
        .await?;
        let mut items: Vec<PersonSummary> = rows
            .into_iter()
            .map(|row| {
                let traits: Map<String, Value> =
                    serde_json::from_str(&row.traits).unwrap_or_default();
                PersonSummary {
                    id: row.id,
                    user_id: row.user_id,
                    display: display(&traits),
                    first_seen: row.first_seen,
                    last_seen: row.last_seen,
                    events: row.events,
                }
            })
            .collect();
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let more = items.len() > limit;
        items.truncate(limit);
        let next_before = if more { items.last().map(|item| item.id) } else { None };
        Ok(PersonPage { items, next_before })
    }
}

/// A person with a page of their timeline.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InPerson {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The person.
    person_id: PersonId,
    /// Events older than this one, from `next_before`.
    #[serde(default)]
    before: Option<i64>,
    /// At most this many events, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}

impl Validate for InPerson {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_limit(&mut errors, self.limit);
        errors.into_result()
    }
}

/// A person, their traits and a page of their events, newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct PersonDetail {
    /// The person ID.
    pub(crate) id: PersonId,
    /// The project.
    pub(crate) project_id: ProjectId,
    /// The site's ID for the user.
    pub(crate) user_id: String,
    /// Traits from `identify`.
    #[schema(value_type = Object)]
    pub(crate) traits: Map<String, Value>,
    /// When their first event or identify arrived.
    pub(crate) first_seen: UnixMillis,
    /// When their latest one arrived.
    pub(crate) last_seen: UnixMillis,
    /// Their events, newest first.
    pub(crate) events: Vec<EventItem>,
    /// Pass as `before` for the next page; none on the last page.
    pub(crate) next_before: Option<i64>,
}

struct PeopleGet;
impl Operation for PeopleGet {
    type Input = InPerson;
    type Output = PersonDetail;
    const ID: &'static str = "analytics.people.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/people/{person_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a person's traits and event timeline";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InPerson) -> Result<PersonDetail, ApiError> {
        let db = cx.platform().db();
        let person = sqlx::query!(
            r#"SELECT project_id AS "project_id: ProjectId", user_id, traits,
                      first_seen AS "first_seen: UnixMillis", last_seen AS "last_seen: UnixMillis"
               FROM analytics_people WHERE workspace_id = ? AND id = ?"#,
            input.workspace_id,
            input.person_id,
        )
        .fetch_optional(db.read())
        .await?
        .ok_or(ApiError::NotFound)?;
        let limit = input.limit.unwrap_or(DEFAULT_PAGE);
        let key = format!("p:{}", input.person_id);
        let before = input.before.unwrap_or(i64::MAX);
        let fetch = i64::from(limit) + 1;
        let rows = sqlx::query!(
            r#"SELECT id AS "id!: i64", ts AS "ts: UnixMillis", name, path, props, source
               FROM analytics_events
               WHERE workspace_id = ? AND project_id = ? AND person = ? AND id < ?
               ORDER BY ts DESC, id DESC LIMIT ?"#,
            input.workspace_id,
            person.project_id,
            key,
            before,
            fetch,
        )
        .fetch_all(db.read())
        .await?;
        let mut events: Vec<EventItem> = rows
            .into_iter()
            .map(|row| EventItem {
                id: row.id,
                ts: row.ts,
                name: row.name,
                path: row.path,
                person_id: Some(input.person_id),
                user_id: Some(person.user_id.clone()),
                props: row
                    .props
                    .and_then(|text| serde_json::from_str(&text).ok())
                    .unwrap_or_default(),
                source: row.source,
            })
            .collect();
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let more = events.len() > limit;
        events.truncate(limit);
        let next_before = if more { events.last().map(|event| event.id) } else { None };
        Ok(PersonDetail {
            id: input.person_id,
            project_id: person.project_id,
            user_id: person.user_id,
            traits: serde_json::from_str(&person.traits).unwrap_or_default(),
            first_seen: person.first_seen,
            last_seen: person.last_seen,
            events,
            next_before,
        })
    }
}

/// Retention cohorts.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetentionQuery {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The first day a cohort may start, UTC.
    from: Day,
    /// The last day a cohort may start, UTC, inclusive.
    to: Day,
    /// The event that counts as a return; default any event.
    #[serde(default)]
    return_event: Option<String>,
}

impl Validate for RetentionQuery {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_range(&mut errors, self.from, self.to);
        if self
            .return_event
            .as_deref()
            .is_some_and(|name| name != crate::beacon::PAGEVIEW && !names::is_event_name(name))
        {
            errors.add("return_event", names::EVENT_NAME_RULE);
        }
        errors.into_result()
    }
}

/// People first seen in one week, and who came back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct Cohort {
    /// The Monday the week starts, UTC.
    pub(crate) week: Day,
    /// People first seen that week.
    pub(crate) people: i64,
    /// For weeks 0 to 8 after: the share of the cohort with a return event
    /// that week, 0 to 1. Null for weeks that haven't begun.
    pub(crate) weeks: Vec<Option<f64>>,
}

/// Weekly retention cohorts, oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct RetentionTable {
    /// The cohorts.
    pub(crate) cohorts: Vec<Cohort>,
}

struct Retention;
impl Operation for Retention {
    type Input = RetentionQuery;
    type Output = RetentionTable;
    const ID: &'static str = "analytics.retention";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/retention";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str =
        "Show weekly retention: of the people first seen each week, the share who came back";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: RetentionQuery) -> Result<RetentionTable, ApiError> {
        let db = cx.platform().db();
        let (scope, _) = site_scope(db, input.workspace_id, input.project_id).await?;
        let first_week = monday(input.from);
        let today = Day::of(cx.now());
        let weeks = i64::try_from(RETENTION_WEEKS).unwrap_or(8);
        let end = monday(input.to).plus_days(7 * (weeks + 1) - 1).min(today);
        let (from, to) = (first_week.to_string(), end.to_string());
        let rows = sqlx::query!(
            r#"SELECT person AS "person!: String", day AS "day!: String", MAX(? IS NULL OR name = ?) AS "returned!: bool"
               FROM analytics_events
               WHERE workspace_id = ? AND project_id = ? AND day >= ? AND day <= ?
                 AND substr(person, 1, 2) <> 'v:'
               GROUP BY person, day"#,
            input.return_event,
            input.return_event,
            scope.workspace_id,
            scope.project_id,
            from,
            to,
        )
        .fetch_all(db.read())
        .await?;
        let days: Vec<(String, Day, bool)> = rows
            .into_iter()
            .filter_map(|row| Some((row.person, row.day.parse().ok()?, row.returned)))
            .collect();
        Ok(RetentionTable { cohorts: cohorts(&days, input.from, input.to, today) })
    }
}

/// The Monday on or before `day`.
pub(crate) fn monday(day: Day) -> Day {
    let since = Day::of(UnixMillis(0)).days_until(day);
    // 1970-01-01 was a Thursday, three days after a Monday.
    day.plus_days(-(since + 3).rem_euclid(7))
}

/// Builds cohorts from each person's active days. A person joins the cohort
/// of the week of their first day in the data, when that day is in
/// `from..=to`.
pub(crate) fn cohorts(days: &[(String, Day, bool)], from: Day, to: Day, today: Day) -> Vec<Cohort> {
    let mut first: HashMap<&str, Day> = HashMap::new();
    for (person, day, _) in days {
        let seen = first.entry(person).or_insert(*day);
        *seen = (*seen).min(*day);
    }
    let mut cohorts: BTreeMap<Day, (i64, Vec<i64>)> = BTreeMap::new();
    let mut returns: HashSet<(&str, usize)> = HashSet::new();
    for (person, day, returned) in days {
        let Some(start) = first.get(person.as_str()).copied().filter(|d| (from..=to).contains(d))
        else {
            continue;
        };
        let week = usize::try_from(monday(start).days_until(*day) / 7).unwrap_or(usize::MAX);
        if *returned && week <= RETENTION_WEEKS {
            returns.insert((person, week));
        }
    }
    for start in first.values().filter(|start| (from..=to).contains(*start)) {
        cohorts.entry(monday(*start)).or_insert_with(|| (0, vec![0; RETENTION_WEEKS + 1])).0 += 1;
    }
    for (person, week) in &returns {
        if let Some(start) = first.get(person)
            && let Some((_, weeks)) = cohorts.get_mut(&monday(*start))
            && let Some(count) = weeks.get_mut(*week)
        {
            *count += 1;
        }
    }
    let as_f64 = |n: i64| f64::from(u32::try_from(n).unwrap_or(u32::MAX));
    cohorts
        .into_iter()
        .map(|(week, (people, counts))| Cohort {
            week,
            people,
            weeks: counts
                .iter()
                .enumerate()
                .map(|(n, &count)| {
                    let begins = week.plus_days(7 * i64::try_from(n).unwrap_or(0));
                    (begins <= today).then(|| as_f64(count) / as_f64(people))
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> Day {
        text.parse().unwrap()
    }

    #[test]
    fn mondays() {
        assert_eq!(monday(day("2026-01-01")), day("2025-12-29"), "a Thursday");
        assert_eq!(monday(day("2025-12-29")), day("2025-12-29"), "a Monday");
        assert_eq!(monday(day("2026-01-04")), day("2025-12-29"), "a Sunday");
    }

    #[test]
    fn cohorts_by_first_week() {
        let row = |p: &str, d: &str, r: bool| (p.to_owned(), day(d), r);
        let days = [
            row("p:a", "2026-01-05", true),
            row("p:a", "2026-01-13", true),
            row("p:b", "2026-01-06", true),
            row("p:b", "2026-01-20", false),
            row("p:c", "2026-01-12", true),
        ];
        let out = cohorts(&days, day("2026-01-05"), day("2026-01-18"), day("2026-01-21"));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].week, day("2026-01-05"));
        assert_eq!(out[0].people, 2);
        assert_eq!(&out[0].weeks[..4], [Some(1.0), Some(0.5), Some(0.0), None]);
        assert_eq!(out[1].week, day("2026-01-12"));
        assert_eq!(&out[1].weeks[..3], [Some(1.0), Some(0.0), None]);
    }
}
