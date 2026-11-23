//! The hook runner: loads scripts, runs them in the sandbox pools, and
//! records each run.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use bizkit_core::Platform;
use bizkit_core::hooks::{HookKind, HookPoint, HookRunner, HookTarget, RunStatus, SyncOutcome};
use bizkit_core::ids::WorkspaceId;
use bizkit_core::jobs::{Enqueue, Job, JobCtx, JobHandler, Lane};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Run, RunCause, RunId, ScriptId};
use crate::redact::Redactor;
use crate::sandbox::{FetchBridge, PoolLimits, RunReport, RunRequest, SandboxPool};
use crate::store::{self, RunRow, ScriptRow};

/// The worker lane script runs use, so they never hold up mail.
pub const EXTENSIONS: Lane = Lane { name: "extensions", concurrency: 2 };
/// The interrupt budget of an async run, fetch waits included.
pub const ASYNC_BUDGET: Duration = Duration::from_secs(10);
/// When the job stops waiting for an async run.
pub const ASYNC_HARD: Duration = Duration::from_secs(15);
/// How long past its budget the request waits for a sync run.
pub const SYNC_SLACK: Duration = Duration::from_millis(100);
/// The largest script source.
pub const MAX_SOURCE_BYTES: usize = 64 << 10;
const MAX_SYNC_INPUT: usize = 64 << 10;
const MAX_ASYNC_INPUT: usize = 256 << 10;
const STACK: usize = 256 << 10;
const SYNC_LIMITS: PoolLimits = PoolLimits { memory: 8 << 20, stack: STACK, queue: 8 };
const ASYNC_LIMITS: PoolLimits = PoolLimits { memory: 32 << 20, stack: STACK, queue: 8 };
const POOL_THREADS: usize = 2;

/// One async script run. The payload holds references only: the input is
/// rebuilt from the event when the run starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunScript {
    /// The script's workspace.
    pub workspace_id: WorkspaceId,
    /// The script.
    pub script_id: ScriptId,
    /// The hook point.
    pub point: String,
    /// The raw event JSON, IDs only.
    pub event: Value,
}

impl Job for RunScript {
    const KIND: &'static str = "extensions.run";
    const MAX_ATTEMPTS: u32 = 3;
    const LANE: Lane = EXTENSIONS;
    const TIMEOUT: Duration = Duration::from_secs(30);
}

#[derive(Debug, Default)]
struct Pools {
    sync: OnceLock<Result<SandboxPool, String>>,
    background: OnceLock<Result<SandboxPool, String>>,
}

/// Runs scripts for every hook point. Clones share the pools, which start
/// on first use, since registration must not spawn threads.
#[derive(Debug, Clone, Default)]
pub struct ScriptRunner {
    pools: Arc<Pools>,
}

fn pool<'a>(
    cell: &'a OnceLock<Result<SandboxPool, String>>,
    name: &'static str,
    limits: PoolLimits,
) -> Result<&'a SandboxPool, String> {
    cell.get_or_init(|| {
        SandboxPool::new(name, POOL_THREADS, limits).map_err(|err| format!("{err:#}"))
    })
    .as_ref()
    .map_err(Clone::clone)
}

fn failed(status: RunStatus, error: impl Into<String>) -> RunReport {
    RunReport {
        status,
        output: None,
        error: Some(error.into()),
        logs: Vec::new(),
        duration: Duration::ZERO,
    }
}

fn json_len(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

/// What [`ScriptRunner::execute`] runs.
#[derive(Debug)]
pub(crate) struct Execution<'a> {
    pub(crate) script: &'a ScriptRow,
    pub(crate) point: &'a HookPoint,
    pub(crate) input: Value,
    pub(crate) cause: RunCause,
    pub(crate) attempt: Option<u32>,
    /// Runs this instead of the stored source, for test runs of an edit.
    pub(crate) source: Option<String>,
}

impl ScriptRunner {
    fn sync_pool(&self) -> Result<&SandboxPool, String> {
        pool(&self.pools.sync, "sync", SYNC_LIMITS)
    }

    fn background_pool(&self) -> Result<&SandboxPool, String> {
        pool(&self.pools.background, "async", ASYNC_LIMITS)
    }

    /// Checks that `source` compiles and exports a default function. It
    /// runs on the async pool, so saving scripts never takes ingest threads.
    pub(crate) async fn check(&self, source: &str) -> Result<(), String> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(format!("is over {} KiB", MAX_SOURCE_BYTES >> 10));
        }
        self.background_pool()?.check(Arc::from(source)).await
    }

    /// Runs a script with its point's capabilities and records the run. The
    /// stored error is also the returned one, redacted.
    pub(crate) async fn execute(&self, platform: &Platform, exec: Execution<'_>) -> Run {
        let Execution { script, point, input, cause, attempt, source } = exec;
        let source: Arc<str> = Arc::from(source.unwrap_or_else(|| script.source.clone()));
        let (report, secrets) = match point.kind {
            HookKind::Sync { budget, output } => {
                (self.run_sync_script(source, input, budget, output).await, BTreeMap::new())
            }
            HookKind::Async { .. } => self.run_async_script(platform, script, source, input).await,
        };
        let run = to_run(platform, script, (cause, attempt), &report, &Redactor::new(&secrets));
        if cause == RunCause::Sync {
            // A sync run answers a request that is waiting under a deadline;
            // queueing for the write lock there could drop the verdict.
            let (platform, workspace_id, run) =
                (platform.clone(), script.workspace_id, run.clone());
            tokio::spawn(async move { record(&platform, workspace_id, &run).await });
        } else {
            record(platform, script.workspace_id, &run).await;
        }
        run
    }

    async fn run_sync_script(
        &self,
        source: Arc<str>,
        input: Value,
        budget: Duration,
        output: fn(&Value) -> Result<(), String>,
    ) -> RunReport {
        if json_len(&input) > MAX_SYNC_INPUT {
            return failed(RunStatus::Error, "the input is over 64 KiB");
        }
        let pool = match self.sync_pool() {
            Ok(pool) => pool,
            Err(err) => return failed(RunStatus::Error, err),
        };
        let req = RunRequest {
            source,
            input,
            budget,
            hard: budget + SYNC_SLACK,
            secrets: None,
            fetch: None,
        };
        let mut report = pool.run(req).await;
        if report.status == RunStatus::Ok
            && let Err(err) = output(report.output.as_ref().unwrap_or(&Value::Null))
        {
            report.status = RunStatus::InvalidOutput;
            report.error = Some(err);
        }
        report
    }

    async fn run_async_script(
        &self,
        platform: &Platform,
        script: &ScriptRow,
        source: Arc<str>,
        input: Value,
    ) -> (RunReport, BTreeMap<String, String>) {
        if json_len(&input) > MAX_ASYNC_INPUT {
            return (failed(RunStatus::Error, "the input is over 256 KiB"), BTreeMap::new());
        }
        let secrets = match store::secret_values(
            platform.db().read(),
            script.workspace_id,
            script.project_id,
        )
        .await
        {
            Ok(secrets) => secrets,
            Err(err) => {
                tracing::warn!(script_id = %script.id, "loading secrets: {err}");
                return (
                    failed(RunStatus::Error, "loading the project's secrets failed"),
                    BTreeMap::new(),
                );
            }
        };
        let pool = match self.background_pool() {
            Ok(pool) => pool,
            Err(err) => return (failed(RunStatus::Error, err), secrets),
        };
        let fetch = FetchBridge::new(
            Arc::clone(&platform.deps().http),
            tokio::runtime::Handle::current(),
            Instant::now() + ASYNC_BUDGET,
        );
        let req = RunRequest {
            source,
            input,
            budget: ASYNC_BUDGET,
            hard: ASYNC_HARD,
            secrets: Some(secrets.clone()),
            fetch: Some(fetch),
        };
        (pool.run(req).await, secrets)
    }
}

fn to_run(
    platform: &Platform,
    script: &ScriptRow,
    (cause, attempt): (RunCause, Option<u32>),
    report: &RunReport,
    redactor: &Redactor,
) -> Run {
    Run {
        id: RunId::new(),
        script_id: script.id,
        cause,
        status: report.status,
        attempt,
        duration_ms: u64::try_from(report.duration.as_millis()).unwrap_or(u64::MAX),
        error: report.error.as_deref().map(|error| redactor.text(error)),
        logs: report.logs.iter().map(|line| redactor.text(line)).collect(),
        output: report.output.clone().map(|output| redactor.json(output)),
        created_at: platform.now(),
    }
}

/// Writes the run row. A failed write is logged and doesn't change the
/// outcome.
async fn record(platform: &Platform, workspace_id: WorkspaceId, run: &Run) {
    if let Err(err) = write_run(platform, workspace_id, run).await {
        tracing::warn!(script_id = %run.script_id, "recording a script run: {err:#}");
    }
}

async fn write_run(
    platform: &Platform,
    workspace_id: WorkspaceId,
    run: &Run,
) -> anyhow::Result<()> {
    let row = RunRow {
        id: run.id,
        workspace_id,
        script_id: run.script_id,
        cause: run.cause.as_str().to_owned(),
        status: run.status.as_str().to_owned(),
        attempt: run.attempt.map(i64::from),
        duration_ms: i64::try_from(run.duration_ms).unwrap_or(i64::MAX),
        error: run.error.clone(),
        logs: serde_json::to_string(&run.logs).context("encoding logs")?,
        output: run
            .output
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .context("encoding output")?,
        created_at: run.created_at,
    };
    let mut tx = platform.db().write().await.context("starting the run transaction")?;
    store::insert_run(&mut tx, &row).await.context("inserting the run")?;
    tx.commit().await
}

#[async_trait]
impl HookRunner for ScriptRunner {
    async fn run_sync(
        &self,
        platform: &Platform,
        point: &HookPoint,
        target: HookTarget,
        input: &Value,
    ) -> SyncOutcome {
        let scripts = match store::enabled_scripts(
            platform.db().read(),
            target.workspace_id,
            target.project_id,
            point.id,
            target.entity.as_deref(),
        )
        .await
        {
            Ok(scripts) => scripts,
            Err(err) => {
                tracing::warn!(point = point.id, "loading the sync script: {err}");
                return SyncOutcome::Failed {
                    status: RunStatus::Error,
                    message: "loading the script failed".to_owned(),
                };
            }
        };
        // The entity's own script wins over the project-wide one. A disabled
        // bound script isn't in `scripts`, so the project-wide one runs: turning
        // off an override goes back to the default rather than to no hook.
        let sync = || scripts.iter().filter(|script| script.sync);
        let Some(script) =
            sync().find(|script| script.target_id.is_some()).or_else(|| sync().next())
        else {
            return SyncOutcome::NoScript;
        };
        let exec = Execution {
            script,
            point,
            input: input.clone(),
            cause: RunCause::Sync,
            attempt: None,
            source: None,
        };
        let run = self.execute(platform, exec).await;
        match run.status {
            RunStatus::Ok => SyncOutcome::Returned(run.output.unwrap_or(Value::Null)),
            status @ (RunStatus::Error
            | RunStatus::Timeout
            | RunStatus::Overloaded
            | RunStatus::InvalidOutput) => {
                SyncOutcome::Failed { status, message: run.error.unwrap_or_default() }
            }
        }
    }

    async fn dispatch(
        &self,
        platform: &Platform,
        point: &HookPoint,
        target: HookTarget,
        event: &Value,
    ) -> anyhow::Result<()> {
        let scripts = store::enabled_scripts(
            platform.db().read(),
            target.workspace_id,
            target.project_id,
            point.id,
            target.entity.as_deref(),
        )
        .await
        .context("loading the async scripts")?;
        let scripts: Vec<&ScriptRow> = scripts.iter().filter(|script| !script.sync).collect();
        if scripts.is_empty() {
            return Ok(());
        }
        let mut tx = platform.db().write().await.context("starting the dispatch transaction")?;
        for script in scripts {
            let job = RunScript {
                workspace_id: script.workspace_id,
                script_id: script.id,
                point: point.id.to_owned(),
                event: event.clone(),
            };
            platform
                .bus()
                .enqueue(&mut tx, &job, Enqueue::default())
                .await
                .with_context(|| format!("enqueueing a run of script {}", script.id))?;
        }
        tx.commit().await
    }
}

/// Runs [`RunScript`] jobs. A failed run fails the attempt, so it retries
/// with backoff and every attempt shows in the run log.
#[derive(Debug, Clone)]
pub(crate) struct RunScriptHandler {
    pub(crate) runner: ScriptRunner,
}

impl JobHandler<RunScript> for RunScriptHandler {
    fn handle(
        &self,
        cx: JobCtx,
        job: RunScript,
    ) -> impl Future<Output = anyhow::Result<()>> + Send {
        let runner = self.runner.clone();
        async move {
            let platform = cx.platform.clone();
            let script = store::find_script(platform.db().read(), job.workspace_id, job.script_id)
                .await
                .context("loading the script")?;
            let Some(script) = script.filter(|script| script.enabled) else { return Ok(()) };
            let Some(point) = platform.hook_point(&job.point) else { return Ok(()) };
            let Some(input) = platform.hook_input(&cx, &job.point, &job.event).await? else {
                return Ok(());
            };
            let exec = Execution {
                script: &script,
                point,
                input,
                cause: RunCause::Event,
                attempt: Some(cx.attempt),
                source: None,
            };
            let run = runner.execute(&platform, exec).await;
            match run.status {
                RunStatus::Ok => Ok(()),
                status @ (RunStatus::Error
                | RunStatus::Timeout
                | RunStatus::Overloaded
                | RunStatus::InvalidOutput) => {
                    Err(anyhow::anyhow!("{status}: {}", run.error.unwrap_or_default()))
                }
            }
        }
    }
}
