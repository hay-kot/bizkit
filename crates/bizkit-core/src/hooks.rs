//! Hook points: places where a module lets workspace scripts run.
//!
//! A module declares its points in [`Module::register`](crate::Module::register)
//! with [`ModuleContext::sync_hook`](crate::ModuleContext::sync_hook) and
//! [`ModuleContext::async_hook`](crate::ModuleContext::async_hook). One module
//! (extensions) provides the [`HookRunner`] that runs scripts. Without one,
//! [`NoHooks`] makes every point inert.
//!
//! - A **sync** point runs inside a request through
//!   [`Platform::call_sync_hook`] and fails open: any failure gives
//!   [`HookResult::Continue`].
//! - An **async** point runs for each event as a job. Core subscribes a
//!   fan-out to the point's event, and the runner queues one run per script.

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use async_trait::async_trait;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::app::Platform;
use crate::ids::{ProjectId, WorkspaceId};
use crate::jobs::{Event, JobCtx, Subscriber};

/// The longest budget a sync point may declare.
pub const MAX_SYNC_BUDGET: Duration = Duration::from_millis(100);

/// How long past a sync point's budget core waits for the runner. The runner
/// answers by its own deadline; this only covers a runner stuck on the
/// database, so it can't hold up the request.
pub const SYNC_GRACE: Duration = Duration::from_secs(1);

/// The subscriber name core uses for its hook fan-outs. Modules can't use it.
pub const HOOKS_SUBSCRIBER: &str = "hooks";

/// A place a module lets scripts run. IDs are owned like job kinds:
/// `<module>.<name>`, such as `forms.pre_validate`.
#[derive(Debug, Clone, Copy)]
pub struct HookPoint {
    /// The point ID.
    pub id: &'static str,
    /// One line for the editor and the hooks list.
    pub summary: &'static str,
    /// A sample input, for test runs and the editor.
    pub sample: fn() -> serde_json::Value,
    /// When the point runs.
    pub kind: HookKind,
}

/// When a hook point runs.
#[derive(Debug, Clone, Copy)]
pub enum HookKind {
    /// Runs in the request and fails open. At most one script per project
    /// and point.
    Sync {
        /// The script's time budget, at most [`MAX_SYNC_BUDGET`].
        budget: Duration,
        /// Checks the shape of a returned value. A failure counts as
        /// [`RunStatus::InvalidOutput`].
        output: fn(&serde_json::Value) -> Result<(), String>,
    },
    /// Runs as a job for each `event`. Any number of scripts per project and
    /// point.
    Async {
        /// The event name, [`Event::NAME`].
        event: &'static str,
    },
}

impl HookKind {
    /// Whether the point runs inside the request.
    #[must_use]
    pub const fn is_sync(&self) -> bool {
        matches!(self, Self::Sync { .. })
    }
}

/// What a script run ended as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// It returned.
    Ok,
    /// It threw, failed to compile or ran out of memory or stack.
    Error,
    /// It ran past its deadline.
    Timeout,
    /// No sandbox was free.
    Overloaded,
    /// It returned a value the point doesn't accept.
    InvalidOutput,
}

impl RunStatus {
    /// The snake-case name, as stored and serialized.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Timeout => "timeout",
            Self::Overloaded => "overloaded",
            Self::InvalidOutput => "invalid_output",
        }
    }
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whose scripts a hook call runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookTarget {
    /// The workspace.
    pub workspace_id: WorkspaceId,
    /// The project inside it.
    pub project_id: ProjectId,
}

/// What a runner reports for a sync call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// No enabled script is attached.
    NoScript,
    /// The script returned this value.
    Returned(serde_json::Value),
    /// The run failed. `status` is never [`RunStatus::Ok`].
    Failed {
        /// How it failed.
        status: RunStatus,
        /// Why, with secrets redacted.
        message: String,
    },
}

/// What a sync call means for the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookResult {
    /// Go on as if no script ran.
    Continue,
    /// The script returned this value, which passed the point's output check.
    Returned(serde_json::Value),
}

/// Runs scripts. At most one module provides it.
#[async_trait]
pub trait HookRunner: Send + Sync + fmt::Debug + 'static {
    /// Runs the sync script for `target` on `point`, if one is attached. It
    /// must answer within the point's budget plus its own sandbox overhead.
    async fn run_sync(
        &self,
        platform: &Platform,
        point: &HookPoint,
        target: HookTarget,
        input: &serde_json::Value,
    ) -> SyncOutcome;

    /// Queues the async scripts for `target` on `point`. `event` is the raw
    /// event JSON; runs rebuild their input from it with
    /// [`Platform::hook_input`]. Core calls it inside a job, so an error
    /// retries the delivery.
    async fn dispatch(
        &self,
        platform: &Platform,
        point: &HookPoint,
        target: HookTarget,
        event: &serde_json::Value,
    ) -> anyhow::Result<()>;
}

/// The runner when no module provides one: no scripts, ever.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHooks;

#[async_trait]
impl HookRunner for NoHooks {
    async fn run_sync(
        &self,
        _platform: &Platform,
        _point: &HookPoint,
        _target: HookTarget,
        _input: &serde_json::Value,
    ) -> SyncOutcome {
        SyncOutcome::NoScript
    }

    async fn dispatch(
        &self,
        _platform: &Platform,
        _point: &HookPoint,
        _target: HookTarget,
        _event: &serde_json::Value,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Builds an async point's target and input from its event. Both run inside
/// jobs, so they can read the database.
pub trait HookSource<E: Event>: Send + Sync + 'static {
    /// Whose scripts the event runs. `None` skips it, such as when the row
    /// it names is gone.
    fn target(
        &self,
        cx: &JobCtx,
        event: &E,
    ) -> impl Future<Output = anyhow::Result<Option<HookTarget>>> + Send;

    /// The script input. `None` skips the run.
    fn input(
        &self,
        cx: &JobCtx,
        event: &E,
    ) -> impl Future<Output = anyhow::Result<Option<serde_json::Value>>> + Send;
}

pub(crate) trait ErasedSource: Send + Sync + fmt::Debug {
    fn input<'a>(
        &'a self,
        cx: &'a JobCtx,
        event: &'a serde_json::Value,
    ) -> BoxFuture<'a, anyhow::Result<Option<serde_json::Value>>>;
}

struct SourceAdapter<E, S>(Arc<S>, PhantomData<fn() -> E>);

impl<E: Event, S> fmt::Debug for SourceAdapter<E, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SourceAdapter").field(&E::NAME).finish()
    }
}

impl<E: Event, S: HookSource<E>> ErasedSource for SourceAdapter<E, S> {
    fn input<'a>(
        &'a self,
        cx: &'a JobCtx,
        event: &'a serde_json::Value,
    ) -> BoxFuture<'a, anyhow::Result<Option<serde_json::Value>>> {
        Box::pin(async move {
            let event =
                E::deserialize(event).with_context(|| format!("decoding a {} event", E::NAME))?;
            self.0.input(cx, &event).await
        })
    }
}

/// Core's subscriber for an async point: finds the target and hands the
/// event to the runner.
pub(crate) struct HookFanout<E, S> {
    point: HookPoint,
    source: Arc<S>,
    event: PhantomData<fn() -> E>,
}

impl<E: Event, S: HookSource<E>> Subscriber<E> for HookFanout<E, S> {
    const NAME: &'static str = HOOKS_SUBSCRIBER;

    fn handle(&self, cx: JobCtx, event: E) -> impl Future<Output = anyhow::Result<()>> + Send {
        let point = self.point;
        let source = Arc::clone(&self.source);
        async move {
            let Some(target) = source.target(&cx, &event).await? else { return Ok(()) };
            let raw = serde_json::to_value(&event)
                .with_context(|| format!("encoding a {} event", E::NAME))?;
            cx.platform.hooks().runner.dispatch(&cx.platform, &point, target, &raw).await
        }
    }
}

/// Hook points, sources and the runner, collected during registration.
#[derive(Debug, Default)]
pub(crate) struct HookRegistry {
    points: Vec<HookPoint>,
    sources: BTreeMap<&'static str, Arc<dyn ErasedSource>>,
    runner: Option<Arc<dyn HookRunner>>,
}

impl HookRegistry {
    pub(crate) fn has_point(&self, id: &str) -> bool {
        self.points.iter().any(|point| point.id == id)
    }

    pub(crate) fn async_point_for(&self, event: &str) -> Option<&HookPoint> {
        self.points
            .iter()
            .find(|point| matches!(point.kind, HookKind::Async { event: e } if e == event))
    }

    pub(crate) fn has_runner(&self) -> bool {
        self.runner.is_some()
    }

    pub(crate) fn add_point(&mut self, point: HookPoint) {
        self.points.push(point);
    }

    /// Adds an async point's source and returns the subscriber that fans its
    /// events out.
    pub(crate) fn add_source<E: Event, S: HookSource<E>>(
        &mut self,
        point: HookPoint,
        source: S,
    ) -> HookFanout<E, S> {
        let source = Arc::new(source);
        let adapter: Arc<dyn ErasedSource> =
            Arc::new(SourceAdapter::<E, S>(Arc::clone(&source), PhantomData));
        self.sources.insert(point.id, adapter);
        HookFanout { point, source, event: PhantomData }
    }

    pub(crate) fn set_runner(&mut self, runner: Arc<dyn HookRunner>) {
        self.runner = Some(runner);
    }

    pub(crate) fn finish(self) -> Hooks {
        Hooks {
            points: self.points,
            sources: self.sources,
            runner: self.runner.unwrap_or_else(|| Arc::new(NoHooks)),
        }
    }
}

/// The hook state the platform holds.
#[derive(Debug)]
pub(crate) struct Hooks {
    pub(crate) points: Vec<HookPoint>,
    sources: BTreeMap<&'static str, Arc<dyn ErasedSource>>,
    pub(crate) runner: Arc<dyn HookRunner>,
}

impl Platform {
    /// Every declared hook point, in registration order.
    #[must_use]
    pub fn hook_points(&self) -> &[HookPoint] {
        &self.hooks().points
    }

    /// The hook point with this ID, if a compiled-in module declares it.
    #[must_use]
    pub fn hook_point(&self, id: &str) -> Option<&HookPoint> {
        self.hooks().points.iter().find(|point| point.id == id)
    }

    /// Runs a sync point's script for `target`. Sync hooks fail open: no
    /// script, a failed run, a value the point's output check refuses, a
    /// runner that takes longer than the budget plus [`SYNC_GRACE`], and an
    /// async `point` all give [`HookResult::Continue`].
    pub async fn call_sync_hook(
        &self,
        point: &HookPoint,
        target: HookTarget,
        input: &serde_json::Value,
    ) -> HookResult {
        let HookKind::Sync { budget, output } = point.kind else {
            tracing::warn!(point = point.id, "call_sync_hook on an async hook point");
            return HookResult::Continue;
        };
        let run = self.hooks().runner.run_sync(self, point, target, input);
        let Ok(outcome) = tokio::time::timeout(budget + SYNC_GRACE, run).await else {
            tracing::warn!(point = point.id, "the hook runner missed its deadline; continuing");
            return HookResult::Continue;
        };
        match outcome {
            SyncOutcome::NoScript => HookResult::Continue,
            SyncOutcome::Returned(value) => match output(&value) {
                Ok(()) => HookResult::Returned(value),
                Err(err) => {
                    tracing::warn!(point = point.id, %err, "the hook runner returned invalid output; continuing");
                    HookResult::Continue
                }
            },
            SyncOutcome::Failed { status, message } => {
                tracing::debug!(point = point.id, %status, %message, "sync hook failed; continuing");
                HookResult::Continue
            }
        }
    }

    /// Rebuilds an async point's script input from a stored event, through
    /// the point's source. `Ok(None)` when the run should skip: the source
    /// says so, or no compiled-in module declares `point`.
    ///
    /// # Errors
    ///
    /// When the event doesn't decode, or the source fails.
    pub async fn hook_input(
        &self,
        cx: &JobCtx,
        point: &str,
        event: &serde_json::Value,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let Some(source) = self.hooks().sources.get(point) else { return Ok(None) };
        source.input(cx, event).await
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::module::{Module, ModuleContext, register_all};

    #[derive(Debug, Serialize, Deserialize)]
    struct Happened;

    impl Event for Happened {
        const NAME: &'static str = "demo.happened";
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Other;

    impl Event for Other {
        const NAME: &'static str = "demo.other";
    }

    struct Source;

    impl<E: Event> HookSource<E> for Source {
        fn target(
            &self,
            _cx: &JobCtx,
            _event: &E,
        ) -> impl Future<Output = anyhow::Result<Option<HookTarget>>> + Send {
            std::future::ready(Ok(None))
        }

        fn input(
            &self,
            _cx: &JobCtx,
            _event: &E,
        ) -> impl Future<Output = anyhow::Result<Option<serde_json::Value>>> + Send {
            std::future::ready(Ok(None))
        }
    }

    struct Quiet;

    impl Subscriber<Happened> for Quiet {
        const NAME: &'static str = HOOKS_SUBSCRIBER;

        fn handle(
            &self,
            _cx: JobCtx,
            _event: Happened,
        ) -> impl Future<Output = anyhow::Result<()>> + Send {
            std::future::ready(Ok(()))
        }
    }

    fn sample() -> serde_json::Value {
        serde_json::json!({})
    }

    #[expect(clippy::unnecessary_wraps, reason = "matches the output check signature")]
    fn any_output(_value: &serde_json::Value) -> Result<(), String> {
        Ok(())
    }

    const fn sync_point(id: &'static str, budget: Duration) -> HookPoint {
        HookPoint { id, summary: "", sample, kind: HookKind::Sync { budget, output: any_output } }
    }

    const fn async_point(id: &'static str, event: &'static str) -> HookPoint {
        HookPoint { id, summary: "", sample, kind: HookKind::Async { event } }
    }

    const SYNC: HookPoint = sync_point("demo.check", Duration::from_millis(50));
    const ASYNC: HookPoint = async_point("demo.after", Happened::NAME);

    type Register = fn(&mut ModuleContext<'_>) -> anyhow::Result<()>;

    #[derive(Debug)]
    struct Demo(&'static str, Register);

    impl Module for Demo {
        fn name(&self) -> &'static str {
            self.0
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            (self.1)(cx)
        }
    }

    fn register(modules: Vec<Demo>) -> anyhow::Result<HookRegistry> {
        let boxed: Vec<Box<dyn Module>> =
            modules.into_iter().map(|module| -> Box<dyn Module> { Box::new(module) }).collect();
        Ok(register_all(&boxed, None)?.hooks)
    }

    fn error(modules: Vec<Demo>) -> String {
        format!("{:#}", register(modules).expect_err("registration should fail"))
    }

    #[test]
    fn modules_declare_points_and_one_runner() {
        let hooks = register(vec![
            Demo("demo", |cx| {
                cx.sync_hook(SYNC)?;
                cx.async_hook::<Happened, _>(ASYNC, Source)
            }),
            Demo("runner", |cx| cx.hook_runner(NoHooks)),
        ])
        .unwrap();
        let ids: Vec<&str> = hooks.points.iter().map(|point| point.id).collect();
        assert_eq!(ids, ["demo.check", "demo.after"], "points in order");
        assert!(hooks.sources.contains_key("demo.after"), "the async point has a source");
        assert!(hooks.has_runner(), "runner registered");
    }

    #[test]
    fn points_must_belong_to_the_module() {
        let err = error(vec![Demo("other", |cx| cx.sync_hook(SYNC))]);
        assert!(err.contains("must start with the module name"), "{err}");
    }

    #[test]
    fn point_ids_are_unique() {
        let err = error(vec![Demo("demo", |cx| {
            cx.sync_hook(SYNC)?;
            cx.sync_hook(SYNC)
        })]);
        assert!(err.contains("already declared"), "{err}");
    }

    #[test]
    fn sync_budgets_are_capped() {
        let err = error(vec![Demo("demo", |cx| {
            cx.sync_hook(sync_point("demo.slow", Duration::from_millis(101)))
        })]);
        assert!(err.contains("at most"), "{err}");
        let zero =
            error(vec![Demo("demo", |cx| cx.sync_hook(sync_point("demo.zero", Duration::ZERO)))]);
        assert!(zero.contains("above zero"), "{zero}");
    }

    #[test]
    fn kinds_must_match_the_call() {
        let err = error(vec![Demo("demo", |cx| cx.sync_hook(ASYNC))]);
        assert!(err.contains("is async"), "{err}");
        let swapped = error(vec![Demo("demo", |cx| cx.async_hook::<Happened, _>(SYNC, Source))]);
        assert!(swapped.contains("is sync"), "{swapped}");
    }

    #[test]
    fn async_points_name_their_event() {
        let err = error(vec![Demo("demo", |cx| cx.async_hook::<Other, _>(ASYNC, Source))]);
        assert!(err.contains("names event demo.happened but subscribes to demo.other"), "{err}");
    }

    #[test]
    fn an_event_has_at_most_one_async_point() {
        let err = error(vec![Demo("demo", |cx| {
            cx.async_hook::<Happened, _>(ASYNC, Source)?;
            cx.async_hook::<Happened, _>(async_point("demo.again", Happened::NAME), Source)
        })]);
        assert!(err.contains("already has async hook point demo.after"), "{err}");
    }

    #[test]
    fn modules_cant_take_the_hooks_subscriber_name() {
        let err = error(vec![Demo("demo", |cx| cx.subscribe::<Happened, _>(Quiet))]);
        assert!(err.contains("core reserves that name"), "{err}");
    }

    #[test]
    fn only_one_runner() {
        let err = error(vec![
            Demo("one", |cx| cx.hook_runner(NoHooks)),
            Demo("two", |cx| cx.hook_runner(NoHooks)),
        ]);
        assert!(err.contains("one is already registered"), "{err}");
    }

    #[test]
    fn run_status_names_match_serde() {
        for status in [
            RunStatus::Ok,
            RunStatus::Error,
            RunStatus::Timeout,
            RunStatus::Overloaded,
            RunStatus::InvalidOutput,
        ] {
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                serde_json::json!(status.as_str()),
                "{status:?}"
            );
        }
    }
}
