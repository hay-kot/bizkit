//! Runs scripts in QuickJS on dedicated OS threads, one fresh runtime per
//! run.
//!
//! The caller always gets an answer by `RunRequest::hard`. The interrupt
//! handler stops ordinary JavaScript at the budget, but native calls such
//! as `Array.prototype.sort` and `JSON.parse` ignore it. When the hard
//! deadline passes on a started run, the pool marks the run abandoned and
//! spawns a replacement thread. The old thread finishes its native call,
//! drops its runtime and exits. The memory limit and the input caps bound
//! how long that takes.

mod fetch;
mod thread;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bizkit_core::hooks::RunStatus;
use tokio::sync::oneshot;

pub use self::fetch::{FETCH_TIMEOUT, FetchBridge, MAX_FETCHES};

/// The OS stack of each sandbox thread. QuickJS checks its own, smaller JS
/// stack limit first.
const THREAD_STACK: usize = 8 << 20;
/// The interrupt budget for loading a script in [`SandboxPool::check`].
const CHECK_BUDGET: Duration = Duration::from_secs(1);
/// The most a script's logs can hold.
pub const MAX_LOG_LINES: usize = 50;
/// The longest log line, in characters. Longer lines are cut.
pub const MAX_LOG_CHARS: usize = 500;
/// The largest serialized return value.
pub const MAX_OUTPUT_BYTES: usize = 64 << 10;
/// The longest error message, in characters.
const MAX_ERROR_CHARS: usize = 2_000;

/// The limits of one pool.
///
/// Memory also bounds how long an abandoned thread stays busy: on a debug
/// build, a native `sort` of 1e6 values (about 30 MiB) runs for about 0.6 s,
/// and 2e6 values no longer fit in 128 MiB.
#[derive(Debug, Clone, Copy)]
pub struct PoolLimits {
    /// The QuickJS heap limit, in bytes.
    pub memory: usize,
    /// The QuickJS stack limit, in bytes.
    pub stack: usize,
    /// How many runs can wait for a thread. Past it, runs are `Overloaded`.
    pub queue: usize,
}

/// One script run.
#[derive(Debug)]
pub struct RunRequest {
    /// The ES module source. Its `default` export is called.
    pub source: Arc<str>,
    /// The first argument, deep-frozen.
    pub input: serde_json::Value,
    /// When the interrupt handler stops the script, from the run's start.
    pub budget: Duration,
    /// When the caller stops waiting, from the call, queueing included.
    pub hard: Duration,
    /// `ctx.secrets`; absent when `None`.
    pub secrets: Option<BTreeMap<String, String>>,
    /// `ctx.fetch`; absent when `None`.
    pub fetch: Option<FetchBridge>,
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    /// What it ended as.
    pub status: RunStatus,
    /// The returned value; `None` for `undefined` or a failure.
    pub output: Option<serde_json::Value>,
    /// Why it failed.
    pub error: Option<String>,
    /// What it logged, capped at [`MAX_LOG_LINES`].
    pub logs: Vec<String>,
    /// From the call to the answer.
    pub duration: Duration,
}

impl RunReport {
    fn failed(status: RunStatus, error: impl Into<String>) -> Self {
        Self {
            status,
            output: None,
            error: Some(error.into()),
            logs: Vec::new(),
            duration: Duration::ZERO,
        }
    }
}

#[derive(Debug)]
enum Work {
    Run(RunRequest),
    Check(Arc<str>),
}

/// Set by the thread when it takes the task.
const STARTED: u8 = 1;
/// Set by the caller when it stops waiting.
const ABANDONED: u8 = 2;

#[derive(Debug)]
struct Task {
    work: Work,
    /// [`STARTED`] and [`ABANDONED`] bits. Both sides use `fetch_or`, so
    /// exactly one of them sees the other's bit and acts on it.
    state: Arc<AtomicU8>,
    reply: oneshot::Sender<RunReport>,
}

#[derive(Debug)]
struct Shared {
    name: &'static str,
    limits: PoolLimits,
    threads: usize,
    receiver: Mutex<Receiver<Task>>,
    /// Abandoned threads still finishing a run.
    overrunning: AtomicUsize,
}

/// A fixed set of sandbox threads with a bounded queue.
#[derive(Debug, Clone)]
pub struct SandboxPool {
    sender: SyncSender<Task>,
    shared: Arc<Shared>,
}

impl SandboxPool {
    /// Spawns `threads` sandbox threads named after `name`.
    ///
    /// # Errors
    ///
    /// When a thread can't be spawned.
    pub fn new(name: &'static str, threads: usize, limits: PoolLimits) -> anyhow::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(limits.queue);
        let shared = Arc::new(Shared {
            name,
            limits,
            threads,
            receiver: Mutex::new(receiver),
            overrunning: AtomicUsize::new(0),
        });
        for _ in 0..threads {
            thread::spawn(&shared)?;
        }
        Ok(Self { sender, shared })
    }

    /// Runs a script. Never blocks the caller's thread, and answers by
    /// `req.hard`: `Timeout` past it, `Overloaded` at once when the queue is
    /// full or too many abandoned threads are still finishing.
    pub async fn run(&self, req: RunRequest) -> RunReport {
        let hard = req.hard;
        let start = Instant::now();
        let mut report = self.submit(Work::Run(req), hard).await;
        report.duration = start.elapsed();
        report
    }

    /// Loads a script and checks that it exports a `default` function,
    /// without calling it.
    ///
    /// # Errors
    ///
    /// The compile error, the load error or the missing export, as text.
    pub async fn check(&self, source: Arc<str>) -> Result<(), String> {
        let report = self.submit(Work::Check(source), CHECK_BUDGET * 2).await;
        match report.status {
            RunStatus::Ok => Ok(()),
            RunStatus::Overloaded => Err("the sandbox is busy; try again".to_owned()),
            RunStatus::Timeout => Err("loading the script took too long".to_owned()),
            RunStatus::Error | RunStatus::InvalidOutput => {
                Err(report.error.unwrap_or_else(|| "the script failed to load".to_owned()))
            }
        }
    }

    async fn submit(&self, work: Work, hard: Duration) -> RunReport {
        if self.shared.overrunning.load(Ordering::SeqCst) >= self.shared.threads * 2 {
            return RunReport::failed(RunStatus::Overloaded, "too many scripts are overrunning");
        }
        let (reply, answer) = oneshot::channel();
        let state = Arc::new(AtomicU8::new(0));
        let task = Task { work, state: Arc::clone(&state), reply };
        match self.sender.try_send(task) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                return RunReport::failed(RunStatus::Overloaded, "the sandbox queue is full");
            }
            Err(TrySendError::Disconnected(_)) => {
                return RunReport::failed(RunStatus::Error, "the sandbox pool is gone");
            }
        }
        match tokio::time::timeout(hard, answer).await {
            Ok(Ok(report)) => report,
            Ok(Err(_)) => RunReport::failed(RunStatus::Error, "the sandbox thread stopped"),
            Err(_) => {
                if state.fetch_or(ABANDONED, Ordering::SeqCst) & STARTED != 0 {
                    self.shared.overrunning.fetch_add(1, Ordering::SeqCst);
                    if let Err(err) = thread::spawn(&self.shared) {
                        tracing::warn!(
                            pool = self.shared.name,
                            "replacing a sandbox thread: {err:#}"
                        );
                    }
                }
                RunReport::failed(RunStatus::Timeout, format!("the script ran past {hard:?}"))
            }
        }
    }
}

fn capped(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

#[cfg(test)]
mod tests;
