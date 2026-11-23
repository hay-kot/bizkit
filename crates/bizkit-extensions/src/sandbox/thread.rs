//! A sandbox thread and one run on it.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};
use std::time::Instant;

use anyhow::Context as _;
use bizkit_core::hooks::RunStatus;
use rquickjs::context::intrinsic::{Date, Eval, Json, MapSet, Promise as PromiseIntrinsic, RegExp};
use rquickjs::{
    Array, CatchResultExt as _, CaughtError, Context, Ctx, Exception, Function, Module, Object,
    Runtime, Value,
};

use super::{
    CHECK_BUDGET, MAX_ERROR_CHARS, MAX_LOG_CHARS, MAX_LOG_LINES, MAX_OUTPUT_BYTES, PoolLimits,
    RunReport, RunRequest, Shared, Task, Work, capped, stdlib,
};

const PRELUDE: &str = include_str!("prelude.js");

/// Starts one sandbox thread on `shared`'s queue.
pub(super) fn spawn(shared: &Arc<Shared>) -> anyhow::Result<()> {
    let shared = Arc::clone(shared);
    std::thread::Builder::new()
        .name(format!("{}-sandbox", shared.name))
        .stack_size(super::THREAD_STACK)
        .spawn(move || serve(&shared))
        .context("spawning a sandbox thread")?;
    Ok(())
}

fn serve(shared: &Shared) {
    loop {
        let task = {
            let receiver = shared.receiver.lock().unwrap_or_else(PoisonError::into_inner);
            receiver.recv()
        };
        let Ok(Task { work, state, reply }) = task else { return };
        if !super::take(&state) {
            continue;
        }
        let report = catch_unwind(AssertUnwindSafe(|| execute(work, shared.limits)))
            .unwrap_or_else(|_| RunReport::failed(RunStatus::Error, "the sandbox panicked"));
        // The caller may have stopped waiting; that is fine.
        drop(reply.send(report));
        if super::finish(&state) {
            shared.overrunning.fetch_sub(1, Ordering::SeqCst);
            return;
        }
    }
}

fn execute(work: Work, limits: PoolLimits) -> RunReport {
    match work {
        Work::Run(req) => sandboxed(&req, limits, Mode::Call),
        Work::Check(source) => {
            let req = RunRequest {
                source,
                input: serde_json::Value::Null,
                budget: CHECK_BUDGET,
                hard: CHECK_BUDGET,
                secrets: None,
                fetch: None,
            };
            sandboxed(&req, limits, Mode::Check)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Call,
}

/// What `ctx.with` hands back: everything owned, nothing tied to `'js`.
type Outcome = Result<Option<String>, String>;

fn sandboxed(req: &RunRequest, limits: PoolLimits, mode: Mode) -> RunReport {
    let start = Instant::now();
    let logs = Rc::new(RefCell::new(Vec::new()));
    let outcome = engine(req, limits, mode, start, &logs);
    let elapsed = start.elapsed();
    let logs = logs.take();
    let failed = |status, error: String| RunReport {
        status,
        output: None,
        error: Some(capped(&error, MAX_ERROR_CHARS)),
        logs: logs.clone(),
        duration: elapsed,
    };
    match outcome {
        Err(_) if elapsed > req.budget => {
            failed(RunStatus::Timeout, format!("the script ran past its {:?} budget", req.budget))
        }
        Err(error) => failed(RunStatus::Error, error),
        Ok(Some(json)) if json.len() > MAX_OUTPUT_BYTES => failed(
            RunStatus::Error,
            format!("the return value is over {} KiB", MAX_OUTPUT_BYTES >> 10),
        ),
        Ok(json) => match json.as_deref().map(serde_json::from_str).transpose() {
            Ok(output) => {
                RunReport { status: RunStatus::Ok, output, error: None, logs, duration: elapsed }
            }
            Err(err) => failed(RunStatus::Error, format!("reading the return value: {err}")),
        },
    }
}

fn engine(
    req: &RunRequest,
    limits: PoolLimits,
    mode: Mode,
    start: Instant,
    logs: &Rc<RefCell<Vec<String>>>,
) -> Outcome {
    let runtime = Runtime::new().map_err(|err| format!("creating a runtime: {err}"))?;
    runtime.set_memory_limit(limits.memory);
    runtime.set_max_stack_size(limits.stack);
    let budget = req.budget;
    runtime.set_interrupt_handler(Some(Box::new(move || start.elapsed() > budget)));
    let context = Context::custom::<(Eval, Date, Json, RegExp, MapSet, PromiseIntrinsic)>(&runtime)
        .map_err(|err| format!("creating a context: {err}"))?;
    context.with(|ctx| {
        let script = load(&ctx, &req.source)?;
        if mode == Mode::Check {
            return Ok(None);
        }
        let args = prelude(&ctx, req, logs)?;
        let input: Value<'_> = caught(&ctx, args.get(0))?;
        let js_ctx: Value<'_> = caught(&ctx, args.get(1))?;
        let returned: Value<'_> = caught(&ctx, script.call((input, js_ctx)))?;
        let settled = match returned.try_into_promise() {
            Ok(promise) => match promise.finish::<Value<'_>>().catch(&ctx) {
                Ok(value) => value,
                Err(CaughtError::Error(rquickjs::Error::WouldBlock)) => {
                    return Err("the script awaited a promise that never settles".to_owned());
                }
                Err(err) => return Err(err.to_string()),
            },
            Err(value) => value,
        };
        let json = caught(&ctx, ctx.json_stringify(settled))?;
        json.map(|text| text.to_string()).transpose().map_err(|err| err.to_string())
    })
}

/// Turns a QuickJS result into text the report can carry.
fn caught<T>(ctx: &Ctx<'_>, result: rquickjs::Result<T>) -> Result<T, String> {
    result.catch(ctx).map_err(|err| match err {
        // QuickJS throws null when it can't even allocate the error.
        CaughtError::Value(value) if value.is_null() => {
            "the script threw null; it may have run out of memory".to_owned()
        }
        err @ (CaughtError::Error(_) | CaughtError::Exception(_) | CaughtError::Value(_)) => {
            err.to_string()
        }
    })
}

/// Evaluates the module and returns its `default` export.
fn load<'js>(ctx: &Ctx<'js>, source: &str) -> Result<Function<'js>, String> {
    let declared = caught(ctx, Module::declare(ctx.clone(), "script", source))?;
    let (module, loaded) = caught(ctx, declared.eval())?;
    caught(ctx, loaded.finish::<()>())?;
    let export: Value<'js> = caught(ctx, module.get("default"))?;
    export.into_function().ok_or_else(|| "the script must export a default function".to_owned())
}

/// Runs the prelude and returns `[input, ctx]`.
fn prelude<'js>(
    ctx: &Ctx<'js>,
    req: &RunRequest,
    logs: &Rc<RefCell<Vec<String>>>,
) -> Result<Array<'js>, String> {
    let setup: Function<'js> = caught(ctx, ctx.eval(PRELUDE))?;
    let host = caught(ctx, host_functions(ctx, req, logs))?;
    let input = serde_json::to_string(&req.input).map_err(|err| err.to_string())?;
    let secrets = req
        .secrets
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|err| err.to_string())?;
    caught(ctx, setup.call((host, input, secrets)))
}

/// Throws a helper's error as a JavaScript `Error`.
fn thrown<T>(ctx: &Ctx<'_>, result: Result<T, String>) -> rquickjs::Result<T> {
    result.map_err(|message| Exception::throw_message(ctx, &message))
}

/// The host functions the prelude wraps. `fetch` is set only when the run
/// has a bridge.
fn host_functions<'js>(
    ctx: &Ctx<'js>,
    req: &RunRequest,
    logs: &Rc<RefCell<Vec<String>>>,
) -> rquickjs::Result<Object<'js>> {
    let host = Object::new(ctx.clone())?;
    let sink = Rc::clone(logs);
    host.set(
        "log",
        Function::new(ctx.clone(), move |line: String| {
            let mut lines = sink.borrow_mut();
            if lines.len() < MAX_LOG_LINES {
                lines.push(capped(&line, MAX_LOG_CHARS));
            }
        }),
    )?;
    if let Some(bridge) = &req.fetch {
        let bridge = bridge.clone();
        host.set(
            "fetch",
            Function::new(ctx.clone(), move |url: String, init: String| bridge.fetch(&url, &init)),
        )?;
    }
    host.set("uuid", Function::new(ctx.clone(), stdlib::uuid))?;
    host.set(
        "digest",
        Function::new(ctx.clone(), |ctx: Ctx<'js>, alg: String, data: String, enc: String| {
            thrown(&ctx, stdlib::digest(&alg, &data, &enc))
        }),
    )?;
    host.set(
        "hmac",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'js>,
             alg: String,
             key: String,
             data: String,
             key_enc: String,
             enc: String| {
                thrown(&ctx, stdlib::hmac(&alg, &key, &data, &key_enc, &enc))
            },
        ),
    )?;
    host.set(
        "timingSafeEqual",
        Function::new(ctx.clone(), |a: String, b: String| stdlib::timing_safe_eq(&a, &b)),
    )?;
    host.set(
        "b64Encode",
        Function::new(ctx.clone(), |text: String, url: bool| stdlib::b64_encode(&text, url)),
    )?;
    host.set(
        "b64Decode",
        Function::new(ctx.clone(), |ctx: Ctx<'js>, text: String, url: bool| {
            thrown(&ctx, stdlib::b64_decode(&text, url))
        }),
    )?;
    host.set("hexEncode", Function::new(ctx.clone(), |text: String| stdlib::hex_encode(&text)))?;
    host.set(
        "hexDecode",
        Function::new(ctx.clone(), |ctx: Ctx<'js>, text: String| {
            thrown(&ctx, stdlib::hex_decode(&text))
        }),
    )?;
    host.set(
        "parseUrl",
        Function::new(ctx.clone(), |ctx: Ctx<'js>, input: String, base: Option<String>| {
            thrown(&ctx, stdlib::parse_url(&input, base.as_deref()))
        }),
    )?;
    host.set(
        "setUrl",
        Function::new(ctx.clone(), |ctx: Ctx<'js>, href: String, part: String, value: String| {
            thrown(&ctx, stdlib::set_url(&href, &part, &value))
        }),
    )?;
    host.set(
        "formEncode",
        Function::new(ctx.clone(), |ctx: Ctx<'js>, pairs: String| {
            thrown(&ctx, stdlib::form_encode(&pairs))
        }),
    )?;
    host.set("formDecode", Function::new(ctx.clone(), |text: String| stdlib::form_decode(&text)))?;
    Ok(host)
}
