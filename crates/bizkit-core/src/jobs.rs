//! Background jobs and events. The `jobs` table is the outbox: jobs and event
//! deliveries are rows written in the same transaction as the change that
//! caused them, so they run if and only if it commits.

mod bus;
pub(crate) mod mail;
mod worker;

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use self::bus::{Bus, Enqueue};
pub use self::mail::SendMail;
pub use self::worker::{RunStats, Worker};
use crate::app::Platform;

/// A group of job kinds with its own worker loops, so slow kinds can't hold
/// up the rest. Lane names are owned like job kinds: the module's name first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lane {
    /// The lane name, such as `extensions`.
    pub name: &'static str,
    /// How many jobs of this lane run at once.
    pub concurrency: usize,
}

impl Lane {
    /// Where jobs run unless they say otherwise: one at a time.
    pub const DEFAULT: Self = Self { name: "default", concurrency: 1 };
}

/// The longest a job may run. A claimed row stays locked this long, so a
/// longer run could be claimed twice after a restart.
pub const MAX_TIMEOUT: Duration = worker::LOCK;

/// A unit of background work.
pub trait Job: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// The kind stored in the row, such as `mail.send`.
    const KIND: &'static str;
    /// Attempts before the job goes `dead`.
    const MAX_ATTEMPTS: u32 = 5;
    /// The worker lane the job runs in.
    const LANE: Lane = Lane::DEFAULT;
    /// How long one attempt may run before it fails and retries. At most [`MAX_TIMEOUT`].
    const TIMEOUT: Duration = MAX_TIMEOUT;
}

/// Runs jobs of one kind.
pub trait JobHandler<J: Job>: Send + Sync + 'static {
    /// Runs `job`. An error or panic retries it with backoff.
    fn handle(&self, cx: JobCtx, job: J) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// Something that happened, delivered to every subscriber as its own job.
pub trait Event: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// The event name, such as `projects.created`.
    const NAME: &'static str;
}

/// Reacts to one event.
pub trait Subscriber<E: Event>: Send + Sync + 'static {
    /// A name unique among the event's subscribers; part of the job kind.
    const NAME: &'static str;
    /// Attempts before the delivery goes `dead`.
    const MAX_ATTEMPTS: u32 = 5;
    /// The worker lane deliveries run in.
    const LANE: Lane = Lane::DEFAULT;
    /// How long one delivery attempt may run. At most [`MAX_TIMEOUT`].
    const TIMEOUT: Duration = MAX_TIMEOUT;
    /// Handles one delivery. An error or panic retries it with backoff.
    fn handle(&self, cx: JobCtx, event: E) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// What a handler knows about its run.
#[derive(Debug, Clone)]
pub struct JobCtx {
    /// The platform.
    pub platform: Platform,
    /// The row id.
    pub job_id: i64,
    /// This attempt, starting at 1.
    pub attempt: u32,
    /// The request that enqueued the job, if any.
    pub request_id: Option<String>,
}

type BoxFuture = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;

pub(crate) trait ErasedHandler: Send + Sync + fmt::Debug {
    fn run(&self, cx: JobCtx, payload: &str) -> BoxFuture;
}

struct JobAdapter<J, H>(Arc<H>, PhantomData<fn() -> J>);

impl<J: Job, H> fmt::Debug for JobAdapter<J, H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("JobAdapter").field(&J::KIND).finish()
    }
}

impl<J: Job, H: JobHandler<J>> ErasedHandler for JobAdapter<J, H> {
    fn run(&self, cx: JobCtx, payload: &str) -> BoxFuture {
        let handler = Arc::clone(&self.0);
        let job = serde_json::from_str::<J>(payload)
            .with_context(|| format!("decoding a {} payload", J::KIND));
        Box::pin(async move { handler.handle(cx, job?).await })
    }
}

struct SubscriberAdapter<E, S>(Arc<S>, PhantomData<fn() -> E>);

impl<E: Event, S: Subscriber<E>> fmt::Debug for SubscriberAdapter<E, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SubscriberAdapter").field(&E::NAME).field(&S::NAME).finish()
    }
}

impl<E: Event, S: Subscriber<E>> ErasedHandler for SubscriberAdapter<E, S> {
    fn run(&self, cx: JobCtx, payload: &str) -> BoxFuture {
        let subscriber = Arc::clone(&self.0);
        let event = serde_json::from_str::<E>(payload)
            .with_context(|| format!("decoding a {} event", E::NAME));
        Box::pin(async move { subscriber.handle(cx, event?).await })
    }
}

/// The job kind for one subscriber's deliveries of one event.
#[must_use]
pub fn delivery_kind(event: &str, subscriber: &str) -> String {
    format!("event:{event}:{subscriber}")
}

/// How the worker runs one job kind.
#[derive(Debug, Clone)]
pub(crate) struct KindEntry {
    pub(crate) handler: Arc<dyn ErasedHandler>,
    pub(crate) max_attempts: u32,
    pub(crate) lane: Lane,
    pub(crate) timeout: Duration,
}

/// Every job kind and event subscription, collected during registration.
#[derive(Debug, Default)]
pub(crate) struct Handlers {
    pub(crate) by_kind: BTreeMap<String, KindEntry>,
    /// Event name → delivery kinds.
    pub(crate) subscribers: BTreeMap<&'static str, Vec<String>>,
}

impl Handlers {
    pub(crate) fn add_job<J: Job, H: JobHandler<J>>(&mut self, handler: H) -> anyhow::Result<()> {
        if self.by_kind.contains_key(J::KIND) {
            bail!("job kind {} is already registered", J::KIND);
        }
        let adapter: Arc<dyn ErasedHandler> =
            Arc::new(JobAdapter::<J, H>(Arc::new(handler), PhantomData));
        let entry = KindEntry {
            handler: adapter,
            max_attempts: J::MAX_ATTEMPTS,
            lane: J::LANE,
            timeout: J::TIMEOUT,
        };
        self.insert(J::KIND.to_owned(), entry)
    }

    fn insert(&mut self, kind: String, entry: KindEntry) -> anyhow::Result<()> {
        let KindEntry { lane, timeout, .. } = entry;
        if timeout > MAX_TIMEOUT || timeout.is_zero() {
            bail!(
                "job kind {kind} has timeout {timeout:?}; it must be above zero and at most {MAX_TIMEOUT:?}"
            );
        }
        if lane.concurrency == 0 {
            bail!("lane {} of job kind {kind} has no concurrency", lane.name);
        }
        let known = self.lanes().into_iter().find(|known| known.name == lane.name);
        if known.is_some_and(|known| known != lane) {
            bail!("lane {} is declared with two different concurrencies", lane.name);
        }
        self.by_kind.insert(kind, entry);
        Ok(())
    }

    /// Every lane, the default one first.
    pub(crate) fn lanes(&self) -> Vec<Lane> {
        let mut lanes = vec![Lane::DEFAULT];
        for entry in self.by_kind.values() {
            if !lanes.iter().any(|lane| lane.name == entry.lane.name) {
                lanes.push(entry.lane);
            }
        }
        lanes
    }

    pub(crate) fn add_subscriber<E: Event, S: Subscriber<E>>(
        &mut self,
        subscriber: S,
    ) -> anyhow::Result<()> {
        let kind = delivery_kind(E::NAME, S::NAME);
        if self.by_kind.contains_key(&kind) {
            bail!("subscriber {} of {} is already registered", S::NAME, E::NAME);
        }
        let adapter: Arc<dyn ErasedHandler> =
            Arc::new(SubscriberAdapter::<E, S>(Arc::new(subscriber), PhantomData));
        let entry = KindEntry {
            handler: adapter,
            max_attempts: S::MAX_ATTEMPTS,
            lane: S::LANE,
            timeout: S::TIMEOUT,
        };
        self.insert(kind.clone(), entry)?;
        self.subscribers.entry(E::NAME).or_default().push(kind);
        Ok(())
    }
}
