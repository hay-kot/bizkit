//! The ingest pipelines under test. Each takes enriched rows and decides when
//! they reach SQLite and when the request gets its answer.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::store::{Row, Store};

/// A pipeline design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Design {
    /// One write transaction per request, answered after commit.
    WriteThrough,
    /// One writer task per file batches whatever is queued into one
    /// transaction (group commit) and answers each request after commit.
    Batched,
    /// One writer task per file buffers rows and flushes on a timer or a
    /// size; requests are answered when the row is queued.
    Buffered,
    /// Buffered's timer and size flushes, but each request is answered after
    /// the flush that holds its row commits.
    Hybrid,
}

/// Queue and flush settings for the batched and buffered designs.
#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    /// Buffered and hybrid: the longest a row waits before a flush.
    pub flush: Duration,
    /// The most rows in one transaction; buffered also flushes at this size.
    pub flush_max: usize,
    /// Queue capacity per writer; a full queue rejects.
    pub queue: usize,
}

/// Why a row wasn't accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    /// The writer's queue is full.
    Full,
    /// The write failed.
    Failed,
}

/// Counters every design keeps.
#[derive(Debug, Default)]
pub struct Counters {
    accepted: AtomicU64,
    rejected_full: AtomicU64,
    failed: AtomicU64,
    committed: AtomicU64,
    batches: AtomicU64,
    batch_max: AtomicU64,
    queue_max: AtomicU64,
}

/// A snapshot of [`Counters`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineStats {
    /// Rows the pipeline answered with success.
    pub accepted: u64,
    /// Rows refused because a queue was full.
    pub rejected_full: u64,
    /// Rows whose write failed.
    pub failed: u64,
    /// Rows committed.
    pub committed: u64,
    /// Write transactions committed.
    pub batches: u64,
    /// Rows in the largest transaction.
    pub batch_max: u64,
    /// The deepest any queue got.
    pub queue_max: u64,
}

impl Counters {
    fn committed(&self, rows: usize) {
        let rows = u64::try_from(rows).unwrap_or(u64::MAX);
        self.committed.fetch_add(rows, Ordering::Relaxed);
        self.batches.fetch_add(1, Ordering::Relaxed);
        self.batch_max.fetch_max(rows, Ordering::Relaxed);
    }

    fn failed(&self, rows: usize) {
        self.failed.fetch_add(u64::try_from(rows).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    fn queued<T>(&self, sender: &mpsc::Sender<T>) {
        let depth = sender.max_capacity().saturating_sub(sender.capacity());
        self.queue_max.fetch_max(u64::try_from(depth).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    fn snapshot(&self) -> PipelineStats {
        PipelineStats {
            accepted: self.accepted.load(Ordering::Relaxed),
            rejected_full: self.rejected_full.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            committed: self.committed.load(Ordering::Relaxed),
            batches: self.batches.load(Ordering::Relaxed),
            batch_max: self.batch_max.load(Ordering::Relaxed),
            queue_max: self.queue_max.load(Ordering::Relaxed),
        }
    }

    fn outcome(&self, result: Result<(), Rejected>) -> Result<(), Rejected> {
        match result {
            Ok(()) => self.accepted.fetch_add(1, Ordering::Relaxed),
            Err(Rejected::Full) => self.rejected_full.fetch_add(1, Ordering::Relaxed),
            Err(Rejected::Failed) => 0,
        };
        result
    }
}

/// A pipeline: rows in, answers out.
#[async_trait::async_trait]
pub trait Pipeline: Send + Sync + fmt::Debug {
    /// Resolves when the design answers the request for `row`.
    async fn submit(&self, row: Row) -> Result<(), Rejected>;
    /// Writes everything queued, then stops the writers.
    async fn shutdown(&self);
    /// The counters so far.
    fn stats(&self) -> PipelineStats;
}

/// Builds the pipeline for `design`.
#[must_use]
pub fn build(design: Design, store: Arc<Store>, tuning: Tuning) -> Arc<dyn Pipeline> {
    match design {
        Design::WriteThrough => Arc::new(WriteThrough { store, counters: Counters::default() }),
        Design::Batched => Arc::new(Writers::<Batch>::new(store, tuning)),
        Design::Buffered => Arc::new(Writers::<Buffer>::new(store, tuning)),
        Design::Hybrid => Arc::new(Writers::<Hybrid>::new(store, tuning)),
    }
}

#[derive(Debug)]
struct WriteThrough {
    store: Arc<Store>,
    counters: Counters,
}

#[async_trait::async_trait]
impl Pipeline for WriteThrough {
    async fn submit(&self, row: Row) -> Result<(), Rejected> {
        let key = self.store.writer_key(row.project);
        let result = if self.store.insert_batch(key, std::slice::from_ref(&row)).await.is_ok() {
            self.counters.committed(1);
            Ok(())
        } else {
            self.counters.failed(1);
            Err(Rejected::Failed)
        };
        self.counters.outcome(result)
    }

    async fn shutdown(&self) {}

    fn stats(&self) -> PipelineStats {
        self.counters.snapshot()
    }
}

/// How a writer task turns queued messages into transactions.
trait Mode: Send + Sync + 'static {
    type Msg: Send + 'static;
    fn run(
        store: Arc<Store>,
        key: u32,
        rx: mpsc::Receiver<Self::Msg>,
        tuning: Tuning,
        counters: Arc<Counters>,
    ) -> impl Future<Output = ()> + Send;
}

/// One writer task per file, created on first use.
struct Writers<M: Mode> {
    store: Arc<Store>,
    tuning: Tuning,
    counters: Arc<Counters>,
    senders: Mutex<HashMap<u32, mpsc::Sender<M::Msg>>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl<M: Mode> fmt::Debug for Writers<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Writers").field("tuning", &self.tuning).finish_non_exhaustive()
    }
}

impl<M: Mode> Writers<M> {
    fn new(store: Arc<Store>, tuning: Tuning) -> Self {
        Self {
            store,
            tuning,
            counters: Arc::new(Counters::default()),
            senders: Mutex::new(HashMap::new()),
            tasks: Mutex::new(Vec::new()),
        }
    }

    async fn sender(&self, key: u32) -> mpsc::Sender<M::Msg> {
        let mut senders = self.senders.lock().await;
        if let Some(sender) = senders.get(&key) {
            return sender.clone();
        }
        let (tx, rx) = mpsc::channel(self.tuning.queue);
        let task = tokio::spawn(M::run(
            Arc::clone(&self.store),
            key,
            rx,
            self.tuning,
            Arc::clone(&self.counters),
        ));
        self.tasks.lock().await.push(task);
        senders.insert(key, tx.clone());
        tx
    }

    async fn enqueue(&self, key: u32, msg: M::Msg) -> Result<(), Rejected> {
        let sender = self.sender(key).await;
        sender.try_send(msg).map_err(|_| Rejected::Full)?;
        self.counters.queued(&sender);
        Ok(())
    }

    async fn stop(&self) {
        self.senders.lock().await.clear();
        let tasks = std::mem::take(&mut *self.tasks.lock().await);
        for task in tasks {
            task.await.unwrap_or_default();
        }
    }
}

/// Group commit: answer after the transaction that holds the row commits.
struct Batch;

impl Mode for Batch {
    type Msg = (Row, oneshot::Sender<bool>);

    async fn run(
        store: Arc<Store>,
        key: u32,
        mut rx: mpsc::Receiver<Self::Msg>,
        tuning: Tuning,
        counters: Arc<Counters>,
    ) {
        while let Some(first) = rx.recv().await {
            let mut batch = vec![first];
            while batch.len() < tuning.flush_max {
                match rx.try_recv() {
                    Ok(msg) => batch.push(msg),
                    Err(_) => break,
                }
            }
            let (rows, answers): (Vec<Row>, Vec<oneshot::Sender<bool>>) = batch.into_iter().unzip();
            let ok = commit(&store, key, &rows, &counters).await;
            for answer in answers {
                answer.send(ok).unwrap_or_default();
            }
        }
    }
}

/// Commits one flush and counts it.
async fn commit(store: &Store, key: u32, rows: &[Row], counters: &Counters) -> bool {
    let ok = store.insert_batch(key, rows).await.is_ok();
    if ok {
        counters.committed(rows.len());
    } else {
        counters.failed(rows.len());
    }
    ok
}

/// Rows to write and the requests to answer once they commit.
type Flush = (Vec<Row>, Vec<oneshot::Sender<bool>>);

/// What a writer task writes to.
struct Writer {
    store: Arc<Store>,
    key: u32,
    counters: Arc<Counters>,
}

/// Writes queued messages every `tuning.flush`, or as soon as
/// `tuning.flush_max` are waiting, and once more when the channel closes.
/// `split` turns the waiting messages into a flush.
async fn flush_on_timer<T>(
    writer: Writer,
    mut rx: mpsc::Receiver<T>,
    tuning: Tuning,
    split: fn(Vec<T>) -> Flush,
) {
    let Writer { store, key, counters } = writer;
    let flush = async |pending: &mut Vec<T>| {
        if pending.is_empty() {
            return;
        }
        let (rows, answers) = split(std::mem::take(pending));
        let ok = commit(&store, key, &rows, &counters).await;
        for answer in answers {
            answer.send(ok).unwrap_or_default();
        }
    };
    let mut pending: Vec<T> = Vec::with_capacity(tuning.flush_max);
    // `interval` would tick at once; the first flush is one window out.
    let mut tick =
        tokio::time::interval_at(tokio::time::Instant::now() + tuning.flush, tuning.flush);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            msg = rx.recv() => {
                let Some(msg) = msg else { break };
                pending.push(msg);
                if pending.len() >= tuning.flush_max {
                    flush(&mut pending).await;
                    tick.reset();
                }
            }
            _ = tick.tick() => flush(&mut pending).await,
        }
    }
    flush(&mut pending).await;
}

/// Interval flush: answer on enqueue, write later.
struct Buffer;

impl Mode for Buffer {
    type Msg = Row;

    async fn run(
        store: Arc<Store>,
        key: u32,
        rx: mpsc::Receiver<Row>,
        tuning: Tuning,
        counters: Arc<Counters>,
    ) {
        let writer = Writer { store, key, counters };
        flush_on_timer(writer, rx, tuning, |rows| (rows, Vec::new())).await;
    }
}

/// Interval flush: answer after the flush that holds the row commits.
struct Hybrid;

impl Mode for Hybrid {
    type Msg = (Row, oneshot::Sender<bool>);

    async fn run(
        store: Arc<Store>,
        key: u32,
        rx: mpsc::Receiver<Self::Msg>,
        tuning: Tuning,
        counters: Arc<Counters>,
    ) {
        let writer = Writer { store, key, counters };
        flush_on_timer(writer, rx, tuning, |batch| batch.into_iter().unzip()).await;
    }
}

/// Modes that answer after commit: the message carries the answer channel.
trait AnswersAfterCommit: Mode<Msg = (Row, oneshot::Sender<bool>)> {}
impl AnswersAfterCommit for Batch {}
impl AnswersAfterCommit for Hybrid {}

#[async_trait::async_trait]
impl<M: AnswersAfterCommit> Pipeline for Writers<M> {
    async fn submit(&self, row: Row) -> Result<(), Rejected> {
        let key = self.store.writer_key(row.project);
        let (tx, rx) = oneshot::channel();
        let result = match self.enqueue(key, (row, tx)).await {
            Ok(()) => match rx.await {
                Ok(true) => Ok(()),
                Ok(false) | Err(_) => Err(Rejected::Failed),
            },
            Err(rejected) => Err(rejected),
        };
        self.counters.outcome(result)
    }

    async fn shutdown(&self) {
        self.stop().await;
    }

    fn stats(&self) -> PipelineStats {
        self.counters.snapshot()
    }
}

#[async_trait::async_trait]
impl Pipeline for Writers<Buffer> {
    async fn submit(&self, row: Row) -> Result<(), Rejected> {
        let key = self.store.writer_key(row.project);
        let result = self.enqueue(key, row).await;
        self.counters.outcome(result)
    }

    async fn shutdown(&self) {
        self.stop().await;
    }

    fn stats(&self) -> PipelineStats {
        self.counters.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::row;
    use crate::store::{Layout, Sync, stored_seqs};

    const TUNING: Tuning =
        Tuning { flush: Duration::from_millis(50), flush_max: 100, queue: 10_000 };

    async fn run(design: Design, layout: Layout) -> (PipelineStats, usize) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), layout, Sync::Normal).await.unwrap());
        let pipeline = build(design, Arc::clone(&store), TUNING);
        let mut submits = Vec::new();
        for seq in 0..500_u64 {
            let pipeline = Arc::clone(&pipeline);
            let project = u32::try_from(seq % 5).unwrap();
            submits.push(tokio::spawn(async move { pipeline.submit(row(project, seq)).await }));
        }
        for submit in submits {
            assert_eq!(submit.await.unwrap(), Ok(()), "accepted");
        }
        pipeline.shutdown().await;
        let stats = pipeline.stats();
        store.close().await;
        (stats, stored_seqs(dir.path()).await.unwrap().len())
    }

    #[tokio::test]
    async fn every_design_stores_every_accepted_row() {
        for design in [Design::WriteThrough, Design::Batched, Design::Buffered, Design::Hybrid] {
            for layout in [Layout::PerProject, Layout::Shared] {
                let (stats, stored) = run(design, layout).await;
                assert_eq!(stats.accepted, 500, "{design:?} {layout:?}");
                assert_eq!(stats.committed, 500, "{design:?} {layout:?}");
                assert_eq!(stored, 500, "{design:?} {layout:?}");
            }
        }
    }

    #[tokio::test]
    async fn batched_commits_before_it_answers() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            Arc::new(Store::open(dir.path(), Layout::PerProject, Sync::Normal).await.unwrap());
        let pipeline = build(Design::Batched, Arc::clone(&store), TUNING);
        pipeline.submit(row(1, 42)).await.unwrap();
        assert_eq!(stored_seqs(dir.path()).await.unwrap(), [42], "visible to a new connection");
        pipeline.shutdown().await;
    }

    #[tokio::test]
    async fn buffered_answers_before_the_flush() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            Arc::new(Store::open(dir.path(), Layout::PerProject, Sync::Normal).await.unwrap());
        let slow = Tuning { flush: Duration::from_secs(60), ..TUNING };
        let pipeline = build(Design::Buffered, Arc::clone(&store), slow);
        pipeline.submit(row(1, 42)).await.unwrap();
        assert!(stored_seqs(dir.path()).await.unwrap().is_empty(), "not written yet");
        pipeline.shutdown().await;
        assert_eq!(stored_seqs(dir.path()).await.unwrap(), [42], "shutdown flushes");
    }

    #[tokio::test]
    async fn hybrid_answers_after_the_flush() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            Arc::new(Store::open(dir.path(), Layout::PerProject, Sync::Normal).await.unwrap());
        let pipeline = build(Design::Hybrid, Arc::clone(&store), TUNING);
        let started = tokio::time::Instant::now();
        pipeline.submit(row(1, 42)).await.unwrap();
        assert!(started.elapsed() >= Duration::from_millis(40), "waited for the timer");
        assert_eq!(stored_seqs(dir.path()).await.unwrap(), [42], "committed before the answer");
        pipeline.shutdown().await;
    }

    #[tokio::test]
    async fn a_full_queue_rejects() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            Arc::new(Store::open(dir.path(), Layout::PerProject, Sync::Normal).await.unwrap());
        let tiny = Tuning { flush: Duration::from_secs(60), flush_max: 1000, queue: 2 };
        let pipeline = build(Design::Buffered, Arc::clone(&store), tiny);
        let results: Vec<_> = submit_in_order(pipeline.as_ref(), 50).await;
        assert!(results.contains(&Err(Rejected::Full)), "{results:?}");
        assert!(pipeline.stats().rejected_full > 0, "counted");
        pipeline.shutdown().await;
    }

    async fn submit_in_order(pipeline: &dyn Pipeline, n: u64) -> Vec<Result<(), Rejected>> {
        let mut out = Vec::new();
        for seq in 0..n {
            out.push(pipeline.submit(row(1, seq)).await);
        }
        out
    }
}
