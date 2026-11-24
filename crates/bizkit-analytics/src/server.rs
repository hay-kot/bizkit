//! `analytics.events.ingest`: product events from a site's own server, with
//! an API key. They skip the bot, host and rate checks, and may identify in
//! every identity mode, because the server knows its users.

use std::sync::Arc;

use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Level, Method, OpCtx, Operation, Scope};
use bizkit_core::time::UnixMillis;
use bizkit_core::validate::{FieldErrors, Validate};
use bizkit_core::{ApiError, Platform};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

use crate::api::Day;
use crate::beacon::{self, Name};
use crate::enrich::{self, Attribution, Who};
use crate::files::Submit;
use crate::ingest::Ingest;
use crate::store::{self, SiteRow};
use crate::write::{Incoming, Origin};

/// The most events per call.
pub(crate) const MAX_BATCH: usize = 100;

/// Events to record.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct IngestEvents {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project whose site gets the events.
    project_id: ProjectId,
    /// 1 to 100 events. They take the time they arrive.
    events: Vec<ServerEvent>,
}

/// One event from a server.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServerEvent {
    /// A custom event name (1-64 letters, digits, or `_ . : -`),
    /// `pageview`, or `$identify` to record traits and link `anon_id` to
    /// `user_id`.
    pub(crate) name: String,
    /// Your ID for the user. Needed for `$identify`; one of `user_id` and
    /// `anon_id` is needed for every event.
    #[serde(default)]
    pub(crate) user_id: Option<String>,
    /// The anonymous ID the snippet stored, to link browser events.
    #[serde(default)]
    pub(crate) anon_id: Option<String>,
    /// The page the event relates to; its path and UTM tags are kept.
    #[serde(default)]
    pub(crate) url: Option<String>,
    /// Event properties: at most 20, with string (256 characters), number
    /// or boolean values.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub(crate) props: Option<Map<String, Value>>,
    /// User traits, for `$identify` only. Same limits as `props`.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub(crate) traits: Option<Map<String, Value>>,
}

impl ServerEvent {
    /// An event bizkit itself sends for `user_id`.
    pub(crate) fn from_server(
        name: &str,
        user_id: &str,
        props: Option<Map<String, Value>>,
        traits: Option<Map<String, Value>>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            user_id: Some(user_id.to_owned()),
            anon_id: None,
            url: None,
            props,
            traits,
        }
    }

    fn check(&self, errors: &mut FieldErrors, at: usize) {
        let field = |name: &str| format!("events[{at}].{name}");
        let name = Name::parse(&self.name);
        if name.is_err() {
            errors.add(field("name"), crate::names::EVENT_NAME_RULE);
        }
        if self.user_id.is_none() && self.anon_id.is_none() {
            errors.add(field("user_id"), "user_id or anon_id is needed");
        }
        if name == Ok(Name::Identify) && self.user_id.is_none() {
            errors.add(field("user_id"), "is needed for $identify");
        }
        if self.traits.is_some() && name != Ok(Name::Identify) {
            errors.add(field("traits"), "only go with $identify");
        }
        if self.user_id.as_deref().is_some_and(|id| !beacon::check_user_id(id)) {
            errors.add(field("user_id"), "must be 1-128 characters with no control characters");
        }
        if self.anon_id.as_deref().is_some_and(|id| !beacon::check_anon_id(id)) {
            errors.add(field("anon_id"), "must be 1-64 letters, digits, - or _");
        }
        if self.url.as_deref().is_some_and(|url| beacon::page_url(url).is_none()) {
            errors.add(field("url"), "must be an http(s) URL of at most 2048 characters");
        }
        for (key, map) in [("props", &self.props), ("traits", &self.traits)] {
            if map.clone().is_some_and(|map| beacon::check_props(map).is_err()) {
                errors.add(
                    field(key),
                    "must have at most 20 keys with string, number or boolean values",
                );
            }
        }
    }
}

impl Validate for IngestEvents {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if !(1..=MAX_BATCH).contains(&self.events.len()) {
            errors.add("events", format!("must have 1 to {MAX_BATCH} events"));
        }
        for (at, event) in self.events.iter().enumerate() {
            event.check(&mut errors, at);
        }
        errors.into_result()
    }
}

/// How many events were accepted.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct Ingested {
    /// Events accepted, counting `$identify` calls. They are written in
    /// batches and show in reports within the flush window (100 ms by
    /// default).
    pub(crate) accepted: usize,
}

pub(crate) struct EventsIngest(pub(crate) Arc<Ingest>);

impl Operation for EventsIngest {
    type Input = IngestEvents;
    type Output = Ingested;
    const ID: &'static str = "analytics.events.ingest";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/events";
    const ACCESS: Access = Access::Workspace(Scope::new("analytics", Level::Write));
    const SUMMARY: &'static str = "Record product events from your server";
    const TAGS: &'static [&'static str] = &["analytics"];
    /// 429 when the project's write queue is full; retry after a second.
    const ERRORS: &'static [StatusCode] = &[StatusCode::TOO_MANY_REQUESTS];

    async fn call(&self, cx: OpCtx, input: IngestEvents) -> Result<Ingested, ApiError> {
        let platform = cx.platform();
        let row = store::find_site(platform.db().read(), input.workspace_id, input.project_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        let accepted = input.events.len();
        record(&self.0, platform, &row, platform.now(), input.events).await?;
        Ok(Ingested { accepted })
    }
}

/// Queues server events, received at `received`, for `row`'s project
/// file. Invalid events fail the call rather than lose their props.
pub(crate) async fn record(
    ingest: &Ingest,
    platform: &Platform,
    row: &SiteRow,
    received: UnixMillis,
    events: Vec<ServerEvent>,
) -> Result<(), ApiError> {
    let mut errors = FieldErrors::default();
    for (at, event) in events.iter().enumerate() {
        event.check(&mut errors, at);
    }
    errors.into_result()?;
    let db = platform.db();
    let site =
        ingest.sites.get(db, &row.site_key, platform.now()).await?.ok_or(ApiError::NotFound)?;
    let salt = ingest.salts.get(db, Day::of(received)).await?;
    let events: Vec<Incoming> = events
        .into_iter()
        .map(|event| incoming(&salt, row.project_id, received, event))
        .collect::<Result<_, _>>()?;
    match ingest.files.submit(platform, site, events).await {
        Ok(()) => Ok(()),
        Err(Submit::Full) => Err(ApiError::RateLimited { retry_after_secs: 1 }),
        Err(Submit::Failed(err)) => Err(ApiError::Internal(err)),
    }
}

fn incoming(
    salt: &[u8; 32],
    project: ProjectId,
    received: UnixMillis,
    event: ServerEvent,
) -> Result<Incoming, ApiError> {
    let name =
        Name::parse(&event.name).map_err(|_| ApiError::BadRequest("bad event name".to_owned()))?;
    let who = match (&event.user_id, &event.anon_id) {
        (Some(user), _) => Who::User(user),
        (None, Some(anon)) => Who::Anon(anon),
        (None, None) => {
            return Err(ApiError::BadRequest("user_id or anon_id is needed".to_owned()));
        }
    };
    let visitor = enrich::visitor(salt, project, who);
    let url = event.url.as_deref().and_then(beacon::page_url);
    let props = event.props.map(beacon::check_props).transpose().ok().flatten();
    let traits = event.traits.map(beacon::check_props).transpose().ok().flatten();
    Ok(Incoming {
        attribution: url
            .as_ref()
            .map_or_else(Attribution::default, |url| enrich::source(url, None)),
        path: url.as_ref().map(enrich::path),
        host: url.as_ref().and_then(|url| url.host_str()).map(str::to_ascii_lowercase),
        country: None,
        agent: None,
        visitor,
        name,
        anon_id: event.anon_id,
        user_id: event.user_id,
        props,
        traits,
        origin: Origin::Server,
        received,
        day: Day::of(received).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribution_needs_a_url() {
        let event = ServerEvent {
            name: "signup".to_owned(),
            user_id: Some("u1".to_owned()),
            anon_id: None,
            url: None,
            props: None,
            traits: None,
        };
        let made = incoming(&[0; 32], ProjectId::new(), UnixMillis(0), event).unwrap();
        assert_eq!(made.attribution, Attribution::default());
        assert_eq!(made.path, None);
    }
}
