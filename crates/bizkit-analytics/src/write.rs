//! Storing one event in its project file: the person, alias and backfill
//! for `$identify`, else the event row and its goal conversions. The
//! buffered writer calls this for each queued event inside its flush
//! transaction; everything that can happen before (parsing, lookups,
//! hashing, the country lookup) happened at ingest.

use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::time::UnixMillis;
use serde_json::{Map, Value};
use sqlx::SqliteConnection;

use crate::api::PersonId;
use crate::beacon::{MAX_PROPS, Name};
use crate::cache::CachedSite;
use crate::enrich::Attribution;
use crate::hooks::GoalConverted;
use crate::report::Scope;
use crate::store::{self, EventRow};
use crate::ua::Agent;

/// How far back identify moves a snippet visitor's anonymous events. The
/// visitor hash is only address and browser, so people behind one NAT with
/// the same browser share it; a short window keeps one person's identify
/// from taking another's day.
const VISITOR_BACKFILL: Duration = Duration::from_mins(30);

/// Where an event came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    /// The snippet.
    Web,
    /// The server ingest operation.
    Server,
}

impl Origin {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Server => "server",
        }
    }
}

/// An event ready to store.
#[derive(Debug, Clone)]
pub(crate) struct Incoming {
    pub(crate) name: Name,
    pub(crate) path: Option<String>,
    pub(crate) host: Option<String>,
    pub(crate) attribution: Attribution,
    pub(crate) country: Option<String>,
    pub(crate) agent: Option<Agent>,
    pub(crate) visitor: i64,
    pub(crate) anon_id: Option<String>,
    pub(crate) user_id: Option<String>,
    pub(crate) props: Option<Map<String, Value>>,
    pub(crate) traits: Option<Map<String, Value>>,
    pub(crate) origin: Origin,
    /// When the server received it. The event's time and day come from
    /// here, not from the flush, so an event queued just before midnight
    /// stays on its day.
    pub(crate) received: UnixMillis,
    /// The UTC day of `received`.
    pub(crate) day: String,
}

/// Stores one event: for `$identify` the person, alias and backfill,
/// else the event row and its goal conversions. Each first conversion is
/// pushed to `converted` for the app side effects.
pub(crate) async fn record_one(
    conn: &mut SqliteConnection,
    site: &CachedSite,
    event: &Incoming,
    converted: &mut Vec<GoalConverted>,
) -> anyhow::Result<()> {
    let person = resolve_person(conn, site, event).await?;
    if event.name == Name::Identify {
        if let Some(person) = person {
            identify(conn, site, event, person).await?;
        }
        return Ok(());
    }
    let visitor_key = format!("v:{}:{}", event.day, event.visitor);
    let key = person.map(person_key).or_else(|| event.anon_id.as_ref().map(|id| format!("a:{id}")));
    let row = event_row(site, event, key.unwrap_or(visitor_key));
    insert(conn, site, &row, converted).await
}

fn person_key(id: PersonId) -> String {
    format!("p:{id}")
}

/// The person an event belongs to: by user ID (made on first sight), else
/// by the anonymous ID's alias.
async fn resolve_person(
    conn: &mut SqliteConnection,
    site: &CachedSite,
    event: &Incoming,
) -> anyhow::Result<Option<PersonId>> {
    let (workspace_id, project_id) = (site.row.workspace_id, site.row.project_id);
    if let Some(user_id) = &event.user_id {
        let id =
            store::upsert_person(&mut *conn, workspace_id, project_id, user_id, event.received)
                .await
                .context("recording a person")?;
        return Ok(Some(id));
    }
    let Some(anon_id) = &event.anon_id else { return Ok(None) };
    Ok(store::aliased_person(&mut *conn, workspace_id, project_id, anon_id).await?)
}

/// Merges traits, links the anonymous ID and moves this browser's earlier
/// anonymous events and conversions to the person: those under the
/// anonymous ID and, for the snippet, the last [`VISITOR_BACKFILL`] under
/// the visitor hash. Events already under a person never move.
async fn identify(
    conn: &mut SqliteConnection,
    site: &CachedSite,
    event: &Incoming,
    person: PersonId,
) -> anyhow::Result<()> {
    let (workspace_id, project_id) = (site.row.workspace_id, site.row.project_id);
    let scope = Scope { workspace_id, project_id };
    if let Some(traits) = event.traits.as_ref().filter(|traits| !traits.is_empty()) {
        let stored = store::person_traits(&mut *conn, workspace_id, person).await?;
        let merged = merge_traits(&stored, traits);
        store::set_person_traits(&mut *conn, workspace_id, person, &merged).await?;
    }
    let to = person_key(person);
    if let Some(anon_id) = &event.anon_id {
        let from = format!("a:{anon_id}");
        store::set_alias(&mut *conn, workspace_id, project_id, anon_id, person).await?;
        store::rekey_events(&mut *conn, scope, &from, &to, UnixMillis(0)).await?;
        store::rekey_conversions(&mut *conn, workspace_id, &from, &to).await?;
    }
    if event.origin == Origin::Web {
        let from = format!("v:{}:{}", event.day, event.visitor);
        let since = event.received.minus(VISITOR_BACKFILL);
        store::rekey_events(&mut *conn, scope, &from, &to, since).await?;
        store::rekey_conversions(&mut *conn, workspace_id, &from, &to).await?;
    }
    Ok(())
}

/// A shallow merge of `new` over the stored traits. Keys past
/// [`MAX_PROPS`] are dropped; existing keys can always change.
fn merge_traits(stored: &str, new: &Map<String, Value>) -> String {
    let mut traits: Map<String, Value> = serde_json::from_str(stored).unwrap_or_default();
    for (key, value) in new {
        if traits.len() < MAX_PROPS || traits.contains_key(key) {
            traits.insert(key.clone(), value.clone());
        }
    }
    Value::Object(traits).to_string()
}

fn event_row(site: &CachedSite, event: &Incoming, person: String) -> EventRow {
    let attribution = &event.attribution;
    EventRow {
        workspace_id: site.row.workspace_id,
        project_id: site.row.project_id,
        ts: event.received,
        day: event.day.clone(),
        name: event.name.as_str().to_owned(),
        path: event.path.clone(),
        host: event.host.clone(),
        referrer_host: attribution.referrer_host.clone(),
        utm_source: attribution.utm_source.clone(),
        utm_medium: attribution.utm_medium.clone(),
        utm_campaign: attribution.utm_campaign.clone(),
        country: event.country.clone(),
        device: event.agent.map(|agent| agent.device.to_owned()),
        browser: event.agent.map(|agent| agent.browser.to_owned()),
        os: event.agent.map(|agent| agent.os.to_owned()),
        visitor: event.visitor,
        person,
        anon_id: event.anon_id.clone(),
        user_id: event.user_id.clone(),
        props: event
            .props
            .as_ref()
            .filter(|props| !props.is_empty())
            .map(|props| Value::Object(props.clone()).to_string()),
        source: event.origin.as_str(),
    }
}

/// Inserts the event row, then records the goal conversions it makes.
async fn insert(
    conn: &mut SqliteConnection,
    site: &CachedSite,
    row: &EventRow,
    converted: &mut Vec<GoalConverted>,
) -> anyhow::Result<()> {
    let event_id = store::insert_event(&mut *conn, row).await.context("inserting an event")?;
    for goal in site.goals.iter().filter(|goal| converts(goal, row)) {
        if store::insert_conversion(&mut *conn, row.workspace_id, goal.id, &row.day, &row.person)
            .await?
        {
            converted.push(GoalConverted {
                workspace_id: row.workspace_id,
                project_id: row.project_id,
                goal_id: goal.id,
                event_id,
            });
        }
    }
    Ok(())
}

pub(crate) fn converts(goal: &store::GoalRow, row: &EventRow) -> bool {
    match goal.kind.as_str() {
        "page" => row.name == crate::beacon::PAGEVIEW && row.path.as_deref() == Some(&goal.matches),
        _ => row.name == goal.matches,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn traits_merge_shallowly_up_to_the_cap() {
        let new = json!({"plan": "pro", "seats": 3});
        let merged =
            merge_traits(r#"{"email":"a@x.test","plan":"free"}"#, new.as_object().unwrap());
        assert_eq!(merged, r#"{"email":"a@x.test","plan":"pro","seats":3}"#);

        let full: Map<String, Value> =
            (0..MAX_PROPS).map(|i| (format!("k{i}"), json!(i))).collect();
        let stored = Value::Object(full).to_string();
        let more = json!({"k0": "changed", "extra": 1});
        let capped: Map<String, Value> =
            serde_json::from_str(&merge_traits(&stored, more.as_object().unwrap())).unwrap();
        assert_eq!(capped.len(), MAX_PROPS);
        assert_eq!(capped["k0"], "changed");
        assert!(!capped.contains_key("extra"));
    }
}
