//! The daily rollup. The first event of a new UTC day for a site queues it
//! (ten minutes later, so the day's last beacons land). It closes every day
//! up to yesterday into `analytics_daily`, one transaction per day, then
//! prunes raw events past retention and old salts.
//!
//! Reruns are safe: rolling a day deletes its rows first, and pruning only
//! touches days already rolled.
//!
//! Daily rows live in the project file and `rolled_through` in the app
//! database. A day's rows commit first; reports read daily rows only up to
//! `rolled_through`, so a crash between the two commits changes no numbers
//! and the rerun replaces the rows.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::db::Db;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::jobs::{Job, JobCtx, JobHandler};
use serde::{Deserialize, Serialize};

use crate::api::{Day, Dimension};
use crate::files::{Files, ProjectDb};
use crate::report::Scope;
use crate::store;

/// How long after the first event of a day the rollup runs.
pub(crate) const DELAY: Duration = Duration::from_mins(10);
/// Raw events kept by default, in days.
pub(crate) const DEFAULT_RETENTION_DAYS: u16 = 90;
/// The shortest raw retention, so funnels and retention have a week.
pub(crate) const MIN_RETENTION_DAYS: u16 = 7;
/// Raw events deleted per transaction, so pruning never holds the writer long.
const PRUNE_CHUNK: i64 = 5_000;

/// Rolls up one site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Rollup {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
}

impl Job for Rollup {
    const KIND: &'static str = "analytics.rollup";
}

/// Runs [`Rollup`] jobs.
#[derive(Debug, Clone)]
pub(crate) struct RollupHandler {
    pub(crate) retention_days: u16,
    pub(crate) files: Arc<Files>,
}

impl JobHandler<Rollup> for RollupHandler {
    async fn handle(&self, cx: JobCtx, job: Rollup) -> anyhow::Result<()> {
        let db = cx.platform.db();
        let today = Day::of(cx.platform.now());
        let scope = Scope { workspace_id: job.workspace_id, project_id: job.project_id };
        let Some(file) = self.files.get(&cx.platform, scope).await? else { return Ok(()) };
        let rolled = roll(db, &file.db, &job, today).await?;
        let cutoff = today.plus_days(-i64::from(self.retention_days.max(MIN_RETENTION_DAYS)));
        let pruned = prune(&file.db, &job, cutoff, rolled).await?;
        let mut tx = db.write().await?;
        store::delete_salts_before(&mut *tx, &today.to_string()).await?;
        tx.commit().await?;
        tracing::debug!(project_id = %job.project_id, ?rolled, pruned, "analytics rollup done");
        Ok(())
    }
}

/// Rolls every closed day after `rolled_through` and returns the new
/// `rolled_through`.
async fn roll(db: &Db, file: &ProjectDb, job: &Rollup, today: Day) -> anyhow::Result<Option<Day>> {
    let Some(site) = store::find_site(db.read(), job.workspace_id, job.project_id).await? else {
        return Ok(None);
    };
    let mut rolled = site.rolled_through.as_deref().and_then(|day| day.parse::<Day>().ok());
    let first = match rolled {
        Some(day) => Some(day.next()),
        None => store::first_event_day(file.read(), job.workspace_id, job.project_id)
            .await?
            .and_then(|day| day.parse().ok()),
    };
    let Some(mut day) = first else { return Ok(rolled) };
    while day < today {
        roll_day(db, file, job, day).await.with_context(|| format!("rolling up {day}"))?;
        rolled = Some(day);
        day = day.next();
    }
    Ok(rolled)
}

async fn roll_day(db: &Db, file: &ProjectDb, job: &Rollup, day: Day) -> anyhow::Result<()> {
    let text = day.to_string();
    let mut tx = file.write().await?;
    store::delete_daily(&mut *tx, job.workspace_id, job.project_id, &text).await?;
    store::insert_daily(&mut *tx, job.workspace_id, job.project_id, "total", &text).await?;
    for dimension in Dimension::ROLLED {
        store::insert_daily(&mut *tx, job.workspace_id, job.project_id, dimension.as_str(), &text)
            .await?;
    }
    tx.commit().await.context("committing a day's rollup")?;
    let mut app_tx = db.write().await?;
    store::set_rolled_through(&mut *app_tx, job.workspace_id, job.project_id, &text).await?;
    app_tx.commit().await
}

/// Deletes raw events and conversions before `cutoff` on rolled days, a
/// chunk per transaction. Returns how many events went.
async fn prune(
    file: &ProjectDb,
    job: &Rollup,
    cutoff: Day,
    rolled: Option<Day>,
) -> anyhow::Result<u64> {
    let Some(rolled) = rolled else { return Ok(0) };
    let before = cutoff.min(rolled.next()).to_string();
    let mut pruned = 0;
    loop {
        let mut tx = file.write().await?;
        let deleted =
            store::prune_events(&mut *tx, job.workspace_id, job.project_id, &before, PRUNE_CHUNK)
                .await?;
        tx.commit().await.context("committing a prune")?;
        pruned += deleted;
        if deleted < PRUNE_CHUNK.unsigned_abs() {
            break;
        }
    }
    let mut tx = file.write().await?;
    store::prune_conversions(&mut *tx, job.workspace_id, &before).await?;
    tx.commit().await.context("committing a conversion prune")?;
    Ok(pruned)
}
