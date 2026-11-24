//! bizkit's own usage, sent to the project `modules.analytics.self_usage`
//! names: each [`UsageReported`] becomes a server event plus an
//! `$identify` with the user's current [`bizkit_core::usage::UsageTraits`].

use std::sync::Arc;

use anyhow::Context as _;
use bizkit_core::db::Db;
use bizkit_core::ids::UserId;
use bizkit_core::jobs::{JobCtx, Subscriber};
use bizkit_core::usage::{self, UsageReported};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::api::Day;
use crate::ingest::Ingest;
use crate::server::{self, ServerEvent};
use crate::store::{self, SiteRow};

/// The `self_usage` part of `modules.analytics`: the project that gets
/// bizkit's usage, by slugs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelfUsageConfig {
    /// The workspace slug.
    pub workspace: String,
    /// The project slug. The project needs an analytics site.
    pub project: String,
}

/// Sends usage reports to the configured project. Registered whether or
/// not the config names one, because `register` must not branch on config;
/// without it a delivery ends at once.
pub(crate) struct SendUsage {
    pub(crate) config: Option<SelfUsageConfig>,
    pub(crate) ingest: Arc<Ingest>,
}

impl Subscriber<UsageReported> for SendUsage {
    const NAME: &'static str = "analytics";

    async fn handle(&self, cx: JobCtx, event: UsageReported) -> anyhow::Result<()> {
        let Some(config) = &self.config else { return Ok(()) };
        let platform = &cx.platform;
        let Some(site) = find_site(platform.db(), config).await? else {
            // A retry can't fix config; the warning tells the operator.
            tracing::warn!(
                workspace = %config.workspace,
                project = %config.project,
                "modules.analytics.self_usage names no project with an analytics site; usage not sent"
            );
            return Ok(());
        };
        let mut events = vec![ServerEvent::from_server(
            event.kind.event_name(),
            &event.user_id.to_string(),
            event.workspace_id.map(|id| {
                Map::from_iter([("workspace_id".to_owned(), Value::from(id.to_string()))])
            }),
            None,
        )];
        for user in std::iter::once(event.user_id).chain(event.affected) {
            events.push(identify(platform.db(), user).await?);
        }
        let now = platform.now();
        // A late delivery keeps its day; one from an earlier day takes now,
        // since reports read that day from the rollup, which won't see it.
        let at = if Day::of(event.at) == Day::of(now) { event.at } else { now };
        server::record(&self.ingest, platform, &site, at, events).await.context("sending usage")
    }
}

/// An `$identify` with `user`'s traits now.
async fn identify(db: &Db, user: UserId) -> anyhow::Result<ServerEvent> {
    let traits = usage::traits(db.read(), user).await?;
    let traits = Map::from_iter([
        ("workspaces".to_owned(), Value::from(traits.workspaces)),
        ("api_keys".to_owned(), Value::from(traits.api_keys)),
    ]);
    Ok(ServerEvent::from_server("$identify", &user.to_string(), None, Some(traits)))
}

async fn find_site(db: &Db, config: &SelfUsageConfig) -> anyhow::Result<Option<SiteRow>> {
    store::find_site_by_slugs(db.read(), &config.workspace, &config.project)
        .await
        .context("finding the self-usage project")
}
