use std::sync::Arc;

use anyhow::{Context as _, bail};

use super::{Event, Handlers, Job};
use crate::db::WriteTx;
use crate::deps::Clock;
use crate::time::UnixMillis;

/// Writes jobs and event deliveries into a write transaction.
#[derive(Debug, Clone)]
pub struct Bus {
    handlers: Arc<Handlers>,
    clock: Arc<dyn Clock>,
}

/// Options for [`Bus::enqueue`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Enqueue {
    /// Run no earlier than this; default now.
    pub run_at: Option<UnixMillis>,
}

impl Bus {
    pub(crate) const fn new(handlers: Arc<Handlers>, clock: Arc<dyn Clock>) -> Self {
        Self { handlers, clock }
    }

    /// Queues `job` in `tx`. It runs once `tx` commits.
    ///
    /// # Errors
    ///
    /// When no handler is registered for `J::KIND` or the insert fails.
    pub async fn enqueue<J: Job>(
        &self,
        tx: &mut WriteTx,
        job: &J,
        opts: Enqueue,
    ) -> anyhow::Result<i64> {
        let entry = self
            .handlers
            .by_kind
            .get(J::KIND)
            .with_context(|| format!("no handler is registered for job {}", J::KIND))?;
        let payload =
            serde_json::to_string(job).with_context(|| format!("encoding a {} job", J::KIND))?;
        let now = self.clock.now();
        let row = NewJob {
            kind: J::KIND,
            payload: &payload,
            max_attempts: entry.max_attempts,
            run_at: opts.run_at.unwrap_or(now),
        };
        insert(tx, row, now).await
    }

    /// Queues one delivery of `event` per subscriber in `tx`.
    ///
    /// # Errors
    ///
    /// When an insert fails.
    pub async fn publish<E: Event>(&self, tx: &mut WriteTx, event: &E) -> anyhow::Result<()> {
        let Some(kinds) = self.handlers.subscribers.get(E::NAME) else { return Ok(()) };
        let payload = serde_json::to_string(event)
            .with_context(|| format!("encoding a {} event", E::NAME))?;
        let now = self.clock.now();
        for kind in kinds {
            let Some(entry) = self.handlers.by_kind.get(kind) else {
                bail!("subscriber kind {kind} has no handler");
            };
            insert(
                tx,
                NewJob { kind, payload: &payload, max_attempts: entry.max_attempts, run_at: now },
                now,
            )
            .await?;
        }
        Ok(())
    }
}

struct NewJob<'a> {
    kind: &'a str,
    payload: &'a str,
    max_attempts: u32,
    run_at: UnixMillis,
}

async fn insert(tx: &mut WriteTx, job: NewJob<'_>, now: UnixMillis) -> anyhow::Result<i64> {
    let NewJob { kind, payload, max_attempts, run_at } = job;
    let request_id = tx.request_id().map(ToOwned::to_owned);
    let max_attempts = i64::from(max_attempts);
    let id = sqlx::query_scalar!(
        "INSERT INTO jobs (kind, payload, status, attempts, max_attempts, run_at, request_id, created_at, updated_at)
         VALUES (?, ?, 'queued', 0, ?, ?, ?, ?, ?) RETURNING id",
        kind,
        payload,
        max_attempts,
        run_at,
        request_id,
        now,
        now,
    )
    .fetch_one(&mut **tx)
    .await
    .with_context(|| format!("inserting a {kind} job"))?;
    tx.mark_jobs_written();
    Ok(id)
}
