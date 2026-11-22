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

use anyhow::{Context as _, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use self::bus::{Bus, Enqueue};
pub use self::mail::SendMail;
pub use self::worker::{RunStats, Worker};
use crate::app::Platform;

/// A unit of background work.
pub trait Job: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// The kind stored in the row, such as `mail.send`.
    const KIND: &'static str;
    /// Attempts before the job goes `dead`.
    const MAX_ATTEMPTS: u32 = 5;
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

/// Every job kind and event subscription, collected during registration.
#[derive(Debug, Default)]
pub(crate) struct Handlers {
    pub(crate) by_kind: BTreeMap<String, (Arc<dyn ErasedHandler>, u32)>,
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
        self.by_kind.insert(J::KIND.to_owned(), (adapter, J::MAX_ATTEMPTS));
        Ok(())
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
        self.by_kind.insert(kind.clone(), (adapter, S::MAX_ATTEMPTS));
        self.subscribers.entry(E::NAME).or_default().push(kind);
        Ok(())
    }
}
