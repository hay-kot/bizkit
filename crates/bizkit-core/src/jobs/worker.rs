use std::ops::AddAssign;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use futures::FutureExt as _;
use tokio_util::sync::CancellationToken;

use super::{Handlers, JobCtx};
use crate::app::Platform;
use crate::time::UnixMillis;

const POLL: Duration = Duration::from_secs(1);
const LOCK: Duration = Duration::from_mins(5);
const MAX_BACKOFF_SECS: u64 = 60 * 60;
const KEEP_DONE: Duration = Duration::from_hours(7 * 24);

/// Runs queued jobs one at a time.
#[derive(Debug, Clone)]
pub struct Worker {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    platform: Platform,
    handlers: Arc<Handlers>,
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
        Self { inner: Arc::new(Inner { platform, handlers }) }
    }

    /// Runs jobs until `shutdown` fires, then returns once the job in flight
    /// finishes. Wakes on commits that wrote jobs, and polls every second for
    /// rows other processes wrote.
    pub async fn run(&self, shutdown: CancellationToken) {
        if let Err(err) = self.requeue_stale().await {
            tracing::error!(error = format!("{err:#}"), "requeuing stale jobs");
        }
        while !shutdown.is_cancelled() {
            match self.tick().await {
                Ok(Some(_)) => continue,
                Ok(None) => {
                    if let Err(err) = self.cleanup().await {
                        tracing::warn!(error = format!("{err:#}"), "deleting old jobs");
                    }
                }
                Err(err) => tracing::error!(error = format!("{err:#}"), "running a job"),
            }
            tokio::select! {
                () = shutdown.cancelled() => break,
                () = self.inner.platform.db().jobs_written() => {}
                () = tokio::time::sleep(POLL) => {}
            }
        }
    }

    /// Runs every job that is due now, then returns what happened. Tests and
    /// `/__test/jobs/drain` use it instead of the background loop.
    ///
    /// # Errors
    ///
    /// When claiming or finishing a job fails. Job failures are counted, not returned.
    pub async fn run_until_idle(&self) -> anyhow::Result<RunStats> {
        let mut stats = RunStats::default();
        while let Some(outcome) = self.tick().await? {
            stats += outcome;
        }
        Ok(stats)
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

    async fn claim(&self) -> anyhow::Result<Option<Claimed>> {
        let now = self.now();
        let locked_until = now.plus(LOCK);
        let mut tx = self.inner.platform.db().write().await?;
        let claimed = sqlx::query_as!(
            Claimed,
            "UPDATE jobs SET status = 'running', locked_until = ?, attempts = attempts + 1, updated_at = ?
             WHERE id = (SELECT id FROM jobs WHERE status = 'queued' AND run_at <= ? ORDER BY run_at, id LIMIT 1)
             RETURNING id, kind, payload, attempts, max_attempts, request_id",
            locked_until,
            now,
            now,
        )
        .fetch_optional(&mut *tx)
        .await
        .context("claiming a job")?;
        tx.commit().await?;
        Ok(claimed)
    }

    async fn tick(&self) -> anyhow::Result<Option<Outcome>> {
        let Some(job) = self.claim().await? else { return Ok(None) };
        let result = match self.inner.handlers.by_kind.get(&job.kind) {
            None => Err(anyhow::anyhow!("no handler for job kind {}", job.kind)),
            Some((handler, _)) => {
                let cx = JobCtx {
                    platform: self.inner.platform.clone(),
                    job_id: job.id,
                    attempt: u32::try_from(job.attempts).unwrap_or(u32::MAX),
                    request_id: job.request_id.clone(),
                };
                match AssertUnwindSafe(handler.run(cx, &job.payload)).catch_unwind().await {
                    Ok(result) => result,
                    Err(panic) => {
                        Err(anyhow::anyhow!("handler panicked: {}", panic_message(&*panic)))
                    }
                }
            }
        };
        let known = self.inner.handlers.by_kind.contains_key(&job.kind);
        let outcome = self.finish(&job, result, known).await?;
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
