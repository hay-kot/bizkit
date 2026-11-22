use std::ops::AddAssign;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use futures::FutureExt as _;
use tokio_util::sync::CancellationToken;

use super::{Handlers, JobCtx, Lane};
use crate::app::Platform;
use crate::time::UnixMillis;

const POLL: Duration = Duration::from_secs(1);
pub(super) const LOCK: Duration = Duration::from_mins(5);
const MAX_BACKOFF_SECS: u64 = 60 * 60;
const KEEP_DONE: Duration = Duration::from_hours(7 * 24);

/// Runs queued jobs. Each lane gets its own loops, so a slow lane never holds
/// up the default one, which runs one job at a time.
#[derive(Debug, Clone)]
pub struct Worker {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    platform: Platform,
    handlers: Arc<Handlers>,
    lanes: Vec<LaneClaim>,
}

/// What one lane's loops claim.
#[derive(Debug)]
struct LaneClaim {
    lane: Lane,
    /// JSON array of job kinds. The default lane claims every kind *not*
    /// listed here, so rows with unknown kinds still reach it and go dead.
    kinds: String,
    default: bool,
}

/// What a drain did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunStats {
    /// Jobs that succeeded.
    pub done: u32,
    /// Jobs that failed and were requeued.
    pub retried: u32,
    /// Jobs that failed for the last time, or had no handler.
    pub dead: u32,
}

impl AddAssign<Outcome> for RunStats {
    fn add_assign(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Done => self.done += 1,
            Outcome::Retried => self.retried += 1,
            Outcome::Dead => self.dead += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Done,
    Retried,
    Dead,
}

struct Claimed {
    id: i64,
    kind: String,
    payload: String,
    attempts: i64,
    max_attempts: i64,
    request_id: Option<String>,
}

impl Worker {
    pub(crate) fn new(platform: Platform, handlers: Arc<Handlers>) -> Self {
        let lanes = handlers
            .lanes()
            .into_iter()
            .map(|lane| {
                let default = lane == Lane::DEFAULT;
                let kinds: Vec<&str> =
                    handlers
                        .by_kind
                        .iter()
                        .filter(|(_, entry)| {
                            if default { entry.lane != Lane::DEFAULT } else { entry.lane == lane }
                        })
                        .map(|(kind, _)| kind.as_str())
                        .collect();
                let kinds = serde_json::Value::from(kinds).to_string();
                LaneClaim { lane, kinds, default }
            })
            .collect();
        Self { inner: Arc::new(Inner { platform, handlers, lanes }) }
    }

    /// Runs jobs until `shutdown` fires, then returns once the jobs in flight
    /// finish. Each lane runs `concurrency` loops. Loops wake on commits that
    /// wrote jobs, and poll every second for rows other processes wrote.
    pub async fn run(&self, shutdown: CancellationToken) {
        if let Err(err) = self.requeue_stale().await {
            tracing::error!(error = format!("{err:#}"), "requeuing stale jobs");
        }
        let mut loops = tokio::task::JoinSet::new();
        for (lane, claim) in self.inner.lanes.iter().enumerate() {
            for slot in 0..claim.lane.concurrency {
                let worker = self.clone();
                let shutdown = shutdown.clone();
                // Only one loop cleans up, so idle loops don't add write transactions.
                let housekeeping = lane == 0 && slot == 0;
                loops.spawn(async move { worker.lane_loop(lane, housekeeping, shutdown).await });
            }
        }
        while loops.join_next().await.is_some() {}
    }

    async fn lane_loop(&self, lane: usize, housekeeping: bool, shutdown: CancellationToken) {
        let Some(lane) = self.inner.lanes.get(lane) else { return };
        let mut wake = self.inner.platform.db().jobs_written();
        while !shutdown.is_cancelled() {
            wake.borrow_and_update();
            match self.tick(lane).await {
                Ok(Some(_)) => continue,
                Ok(None) if housekeeping => {
                    if let Err(err) = self.cleanup().await {
                        tracing::warn!(error = format!("{err:#}"), "deleting old jobs");
                    }
                }
                Ok(None) => {}
                Err(err) => tracing::error!(error = format!("{err:#}"), "running a job"),
            }
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = wake.changed() => {}
                () = tokio::time::sleep(POLL) => {}
            }
        }
    }

    /// Runs every job that is due now in every lane, one at a time, then
    /// returns what happened. Tests and `/__test/jobs/drain` use it instead of
    /// the background loops.
    ///
    /// # Errors
    ///
    /// When claiming or finishing a job fails. Job failures are counted, not returned.
    pub async fn run_until_idle(&self) -> anyhow::Result<RunStats> {
        let mut stats = RunStats::default();
        loop {
            let mut ran = false;
            for lane in &self.inner.lanes {
                while let Some(outcome) = self.tick(lane).await? {
                    stats += outcome;
                    ran = true;
                }
            }
            if !ran {
                return Ok(stats);
            }
        }
    }

    fn now(&self) -> UnixMillis {
        self.inner.platform.now()
    }

    async fn requeue_stale(&self) -> anyhow::Result<()> {
        let now = self.now();
        let mut tx = self.inner.platform.db().write().await?;
        let result = sqlx::query!(
            "UPDATE jobs SET status = 'queued', locked_until = NULL, updated_at = ?
             WHERE status = 'running' AND locked_until < ?",
            now,
            now,
        )
        .execute(&mut *tx)
        .await
        .context("requeuing stale jobs")?;
        tx.commit().await?;
        if result.rows_affected() > 0 {
            tracing::warn!(
                count = result.rows_affected(),
                "requeued jobs a stopped worker left running"
            );
        }
        Ok(())
    }

    async fn cleanup(&self) -> anyhow::Result<()> {
        let cutoff = self.now().minus(KEEP_DONE);
        let mut tx = self.inner.platform.db().write().await?;
        sqlx::query!("DELETE FROM jobs WHERE status = 'done' AND updated_at < ?", cutoff)
            .execute(&mut *tx)
            .await
            .context("deleting done jobs")?;
        tx.commit().await
    }

    /// Whether `lane` has a due job, checked on the read pool so idle loops
    /// don't take the write lock.
    async fn has_due(&self, lane: &LaneClaim, now: UnixMillis) -> anyhow::Result<bool> {
        let read = self.inner.platform.db().read();
        let found = if lane.default {
            sqlx::query_scalar!(
                "SELECT 1 AS \"found!: i64\" FROM jobs WHERE status = 'queued' AND run_at <= ?
                 AND kind NOT IN (SELECT value FROM json_each(?)) LIMIT 1",
                now,
                lane.kinds,
            )
            .fetch_optional(read)
            .await
        } else {
            sqlx::query_scalar!(
                "SELECT 1 AS \"found!: i64\" FROM jobs WHERE status = 'queued' AND run_at <= ?
                 AND kind IN (SELECT value FROM json_each(?)) LIMIT 1",
                now,
                lane.kinds,
            )
            .fetch_optional(read)
            .await
        };
        Ok(found.context("checking for due jobs")?.is_some())
    }

    async fn claim(&self, lane: &LaneClaim) -> anyhow::Result<Option<Claimed>> {
        let now = self.now();
        if !self.has_due(lane, now).await? {
            return Ok(None);
        }
        let locked_until = now.plus(LOCK);
        let mut tx = self.inner.platform.db().write().await?;
        let claimed = if lane.default {
            sqlx::query_as!(
                Claimed,
                "UPDATE jobs SET status = 'running', locked_until = ?, attempts = attempts + 1, updated_at = ?
                 WHERE id = (SELECT id FROM jobs WHERE status = 'queued' AND run_at <= ?
                             AND kind NOT IN (SELECT value FROM json_each(?)) ORDER BY run_at, id LIMIT 1)
                 RETURNING id, kind, payload, attempts, max_attempts, request_id",
                locked_until,
                now,
                now,
                lane.kinds,
            )
            .fetch_optional(&mut *tx)
            .await
        } else {
            sqlx::query_as!(
                Claimed,
                "UPDATE jobs SET status = 'running', locked_until = ?, attempts = attempts + 1, updated_at = ?
                 WHERE id = (SELECT id FROM jobs WHERE status = 'queued' AND run_at <= ?
                             AND kind IN (SELECT value FROM json_each(?)) ORDER BY run_at, id LIMIT 1)
                 RETURNING id, kind, payload, attempts, max_attempts, request_id",
                locked_until,
                now,
                now,
                lane.kinds,
            )
            .fetch_optional(&mut *tx)
            .await
        }
        .context("claiming a job")?;
        tx.commit().await?;
        Ok(claimed)
    }

    async fn tick(&self, lane: &LaneClaim) -> anyhow::Result<Option<Outcome>> {
        let Some(job) = self.claim(lane).await? else { return Ok(None) };
        let entry = self.inner.handlers.by_kind.get(&job.kind);
        let result = match entry {
            None => Err(anyhow::anyhow!("no handler for job kind {}", job.kind)),
            Some(entry) => {
                let cx = JobCtx {
                    platform: self.inner.platform.clone(),
                    job_id: job.id,
                    attempt: u32::try_from(job.attempts).unwrap_or(u32::MAX),
                    request_id: job.request_id.clone(),
                };
                let run = AssertUnwindSafe(entry.handler.run(cx, &job.payload)).catch_unwind();
                match tokio::time::timeout(entry.timeout, run).await {
                    Ok(Ok(result)) => result,
                    Ok(Err(panic)) => {
                        Err(anyhow::anyhow!("handler panicked: {}", panic_message(&*panic)))
                    }
                    Err(_) => Err(anyhow::anyhow!("timed out after {:?}", entry.timeout)),
                }
            }
        };
        let outcome = self.finish(&job, result, entry.is_some()).await?;
        Ok(Some(outcome))
    }

    async fn finish(
        &self,
        job: &Claimed,
        result: anyhow::Result<()>,
        known: bool,
    ) -> anyhow::Result<Outcome> {
        let now = self.now();
        let mut tx = self.inner.platform.db().write().await?;
        let outcome = match result {
            Ok(()) => {
                sqlx::query!(
                    "UPDATE jobs SET status = 'done', locked_until = NULL, last_error = NULL, updated_at = ? WHERE id = ?",
                    now,
                    job.id,
                )
                .execute(&mut *tx)
                .await
                .context("marking a job done")?;
                Outcome::Done
            }
            Err(err) => {
                let message = format!("{err:#}");
                if !known || job.attempts >= job.max_attempts {
                    tracing::error!(job_id = job.id, kind = %job.kind, attempts = job.attempts, error = %message, "job is dead");
                    sqlx::query!(
                        "UPDATE jobs SET status = 'dead', locked_until = NULL, last_error = ?, updated_at = ? WHERE id = ?",
                        message,
                        now,
                        job.id,
                    )
                    .execute(&mut *tx)
                    .await
                    .context("marking a job dead")?;
                    Outcome::Dead
                } else {
                    let run_at = now.plus(backoff(job.attempts));
                    tracing::warn!(job_id = job.id, kind = %job.kind, attempts = job.attempts, error = %message, "job failed; retrying");
                    sqlx::query!(
                        "UPDATE jobs SET status = 'queued', locked_until = NULL, last_error = ?, run_at = ?, updated_at = ?
                         WHERE id = ?",
                        message,
                        run_at,
                        now,
                        job.id,
                    )
                    .execute(&mut *tx)
                    .await
                    .context("requeuing a job")?;
                    Outcome::Retried
                }
            }
        };
        tx.commit().await?;
        Ok(outcome)
    }
}

/// `min(2^attempts s, 1 h)`.
fn backoff(attempts: i64) -> Duration {
    let exponent = u32::try_from(attempts.clamp(0, 12)).unwrap_or(12);
    Duration::from_secs(2_u64.pow(exponent).min(MAX_BACKOFF_SECS))
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(backoff(1), Duration::from_secs(2), "first retry");
        assert_eq!(backoff(3), Duration::from_secs(8), "third retry");
        assert_eq!(backoff(40), Duration::from_secs(MAX_BACKOFF_SECS), "capped at an hour");
    }
}
