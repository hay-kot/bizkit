//! Insights: one typed query over a project's events or people, saved
//! insights that keep one, and dashboards that line them up.

mod query;
mod run;
mod saved;

use std::sync::Arc;

use bizkit_core::ApiError;
use bizkit_core::ModuleContext;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Level, Method, OpCtx, Operation, Scope as AccessScope};
use bizkit_core::validate::{FieldErrors, Validate};
use serde::Deserialize;
use utoipa::ToSchema;

pub(crate) use self::query::InsightQuery;
use self::query::InsightResult;
#[cfg(feature = "test-hooks")]
pub(crate) use self::saved::seed_dashboard;
use crate::api::Day;
use crate::files::Files;
use crate::stats::{check_range, site_scope};

const READ: Access = Access::Workspace(AccessScope::new("analytics", Level::Read));
const WRITE: Access = Access::Workspace(AccessScope::new("analytics", Level::Write));
const TAGS: &[&str] = &["analytics"];

pub(crate) fn register(
    cx: &mut ModuleContext<'_>,
    files: &Arc<Files>,
    retention_days: u16,
) -> anyhow::Result<()> {
    cx.op(InsightsQuery { files: Arc::clone(files), retention_days })?;
    saved::register(cx)
}

/// An insight query to run over a range.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunInsight {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The first day, UTC. People insights ignore the range.
    from: Day,
    /// The last day, UTC, inclusive. At most 400 days after `from`.
    to: Day,
    /// What to count.
    query: InsightQuery,
}

impl Validate for RunInsight {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_range(&mut errors, self.from, self.to);
        self.query.check(&mut errors, "query");
        errors.into_result()
    }
}

struct InsightsQuery {
    files: Arc<Files>,
    retention_days: u16,
}

impl Operation for InsightsQuery {
    type Input = RunInsight;
    type Output = InsightResult;
    const ID: &'static str = "analytics.insights.query";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/insights/query";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Run an insight query: events by total, users, share of active \
         users or per-user count, or people by trait, with filters and an optional breakdown";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: RunInsight) -> Result<InsightResult, ApiError> {
        let platform = cx.platform();
        let (scope, _) = site_scope(platform.db(), input.workspace_id, input.project_id).await?;
        let file = self.files.for_op(platform, scope).await?;
        let oldest_raw = Day::of(cx.now()).plus_days(-i64::from(self.retention_days));
        let range = run::Range { from: input.from, to: input.to, oldest_raw };
        Ok(run::run(file.db.read(), scope, range, &input.query).await?)
    }
}
