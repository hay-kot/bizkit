use std::time::{Duration, Instant};

use bizkit_testkit::FakeHttp;
use http::{Method, StatusCode};
use serde_json::json;

use super::*;

const MIB: usize = 1 << 20;
const LIMITS: PoolLimits = PoolLimits { memory: 8 * MIB, stack: 256 << 10, queue: 4 };
const BUDGET: Duration = Duration::from_millis(50);
const HARD: Duration = Duration::from_millis(150);
/// A native sort that ignores the interrupt for well over `HARD`.
const LONG_SORT: &str = "export default () => { const a = JSON.parse('[' + '7,3,9,1,'.repeat(2.5e5) + '1]'); a.sort(); return 1 }";

fn pool(threads: usize, limits: PoolLimits) -> SandboxPool {
    SandboxPool::new("test", threads, limits).unwrap()
}

fn request(source: &str) -> RunRequest {
    RunRequest {
        source: Arc::from(source),
        input: json!({ "name": "Ada", "tags": ["a"] }),
        budget: BUDGET,
        hard: HARD,
        secrets: None,
        fetch: None,
        mail: None,
    }
}

async fn run(source: &str) -> RunReport {
    pool(1, LIMITS).run(request(source)).await
}

fn big_pool(threads: usize) -> SandboxPool {
    pool(threads, PoolLimits { memory: 128 * MIB, ..LIMITS })
}

#[tokio::test]
async fn returns_a_value() {
    let report =
        run("export default (input) => ({ hello: input.name, n: input.tags.length })").await;
    assert_eq!(report.status, RunStatus::Ok, "{report:?}");
    assert_eq!(report.output, Some(json!({ "hello": "Ada", "n": 1 })), "output");
}

#[tokio::test]
async fn awaits_async_functions() {
    let report = run("export default async (input) => { await null; return input.name }").await;
    assert_eq!(report.output, Some(json!("Ada")), "{report:?}");
    let nothing = run("export default () => {}").await;
    assert_eq!((nothing.status, nothing.output), (RunStatus::Ok, None), "undefined");
}

#[tokio::test]
async fn thrown_errors_are_errors() {
    let report = run("export default () => { throw new Error('nope') }").await;
    assert_eq!(report.status, RunStatus::Error, "{report:?}");
    assert!(report.error.unwrap().contains("nope"), "message kept");
    let stuck = run("export default () => new Promise(() => {})").await;
    assert!(stuck.error.unwrap().contains("never settles"), "a promise that never settles");
}

#[tokio::test]
async fn loops_time_out() {
    for source in [
        "export default () => { for (;;) {} }",
        "export default () => { try { for (;;) {} } catch {} return 1 }",
        "for (;;) {} export default () => 1",
    ] {
        let start = Instant::now();
        let report = run(source).await;
        assert_eq!(report.status, RunStatus::Timeout, "{source}: {report:?}");
        assert!(start.elapsed() < HARD + Duration::from_millis(100), "{source}: answered in time");
    }
}

#[tokio::test]
async fn memory_and_stack_are_limited() {
    // A loaded machine can take longer than BUDGET to fill 8 MiB.
    let unhurried = |source| RunRequest {
        budget: Duration::from_secs(5),
        hard: Duration::from_secs(6),
        ..request(source)
    };
    let pool = pool(1, LIMITS);
    let oom =
        pool.run(unhurried("export default () => { const a = []; for (;;) a.push({ a }) }")).await;
    assert_eq!(oom.status, RunStatus::Error, "{oom:?}");
    let deep = pool.run(unhurried("const f = (n) => f(n + 1); export default () => f(0)")).await;
    assert_eq!(deep.status, RunStatus::Error, "{deep:?}");
}

#[tokio::test]
async fn native_overruns_meet_the_hard_deadline_and_the_thread_is_replaced() {
    let pool = big_pool(1);
    let start = Instant::now();
    let report = pool.run(request(LONG_SORT)).await;
    assert_eq!(report.status, RunStatus::Timeout, "{report:?}");
    assert!(start.elapsed() < HARD + Duration::from_millis(100), "the caller got its answer");
    let next = pool.run(request("export default () => 2")).await;
    assert_eq!(next.output, Some(json!(2)), "a replacement thread ran the next script: {next:?}");
}

#[test]
fn the_handshake_counts_each_overrun_once() {
    let finished_first = AtomicU8::new(0);
    assert!(take(&finished_first));
    assert!(!finish(&finished_first), "the thread serves on");
    assert_eq!(abandon(&finished_first), GaveUp::Finished, "the caller takes the report");

    let abandoned_first = AtomicU8::new(0);
    assert!(take(&abandoned_first));
    assert_eq!(abandon(&abandoned_first), GaveUp::Overrunning);
    assert!(finish(&abandoned_first), "the counted thread exits");

    let queued = AtomicU8::new(0);
    assert_eq!(abandon(&queued), GaveUp::Queued);
    assert!(!take(&queued), "the thread skips it");
}

#[tokio::test]
async fn overruns_past_the_cap_are_overloaded() {
    let pool = big_pool(1);
    for _ in 0..2 {
        assert_eq!(pool.run(request(LONG_SORT)).await.status, RunStatus::Timeout, "overruns");
    }
    let report = pool.run(request("export default () => 1")).await;
    assert_eq!(report.status, RunStatus::Overloaded, "two abandoned threads is the cap for one");
}

#[tokio::test]
async fn a_full_queue_is_overloaded() {
    let pool = big_pool(1);
    let mut long = request(LONG_SORT);
    long.hard = Duration::from_secs(30);
    let busy = tokio::spawn({
        let pool = pool.clone();
        async move { pool.run(long).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut queued = Vec::new();
    for _ in 0..LIMITS.queue {
        let pool = pool.clone();
        queued.push(tokio::spawn(async move { pool.run(request("export default () => 1")).await }));
    }
    tokio::time::sleep(Duration::from_millis(20)).await;
    let report = pool.run(request("export default () => 1")).await;
    assert_eq!(report.status, RunStatus::Overloaded, "{report:?}");
    busy.abort();
    for task in queued {
        task.abort();
    }
}

#[tokio::test]
async fn logs_and_output_are_capped() {
    let report = run(
        "export default () => { for (let i = 0; i < 80; i++) console.log('x'.repeat(900), i) }",
    )
    .await;
    assert_eq!(report.logs.len(), MAX_LOG_LINES, "lines capped");
    assert_eq!(report.logs[0].chars().count(), MAX_LOG_CHARS, "line length capped");
    let logged =
        run("export default (input, ctx) => { ctx.log('a', { b: 1 }, [2]); return 0 }").await;
    assert_eq!(logged.logs, ["a {\"b\":1} [2]"], "ctx.log formats like console");
    let big = run("export default () => 'x'.repeat(70000)").await;
    assert_eq!(big.status, RunStatus::Error, "{big:?}");
    assert!(big.error.unwrap().contains("over 64 KiB"), "output cap");
}

#[tokio::test]
async fn code_generation_and_capabilities_are_gone() {
    let report = run(
        "export default (input, ctx) => [typeof eval, typeof Function, typeof ctx.fetch, typeof ctx.secrets, typeof ctx.mail, typeof Proxy, typeof Uint8Array, typeof WeakRef, typeof setTimeout]",
    )
    .await;
    assert_eq!(
        report.output,
        Some(json!(vec!["undefined"; 9])),
        "nothing but the granted ctx: {report:?}"
    );
}

#[tokio::test]
async fn input_and_ctx_are_frozen() {
    let report = run(
        "export default (input, ctx) => { 'use strict'; const out = []; for (const f of [() => { input.name = 'x' }, () => { input.tags.push('b') }, () => { ctx.log = null }]) { try { f(); out.push('wrote') } catch { out.push('threw') } } return out }",
    )
    .await;
    assert_eq!(report.output, Some(json!(["threw", "threw", "threw"])), "{report:?}");
}

#[tokio::test]
async fn check_refuses_bad_scripts() {
    let pool = pool(1, LIMITS);
    assert_eq!(pool.check(Arc::from("export default () => 1")).await, Ok(()), "valid");
    let syntax = pool.check(Arc::from("export default (")).await.unwrap_err();
    assert!(syntax.contains("unexpected token"), "{syntax}");
    let missing = pool.check(Arc::from("export const x = 1")).await.unwrap_err();
    assert!(missing.contains("default function"), "{missing}");
    let thrown = pool.check(Arc::from("throw new Error('boom'); export default () => 1")).await;
    assert!(thrown.unwrap_err().contains("boom"), "top-level throw");
}

#[tokio::test]
async fn crypto_and_encoding_globals_work_without_capabilities() {
    let report = run(
        "export default () => [crypto.hmac('sha256', 'Jefe', 'what do ya want for nothing?'), crypto.digest('sha1', 'abc', { encoding: 'base64' }), crypto.hmac('sha1', encoding.hexEncode('Jefe'), 'what do ya want for nothing?', { keyEncoding: 'hex', encoding: 'base64' }), crypto.timingSafeEqual('a', 'a'), crypto.timingSafeEqual('a', 'b'), crypto.randomUUID().length, encoding.base64Encode('héllo'), encoding.base64Decode(encoding.base64Encode('héllo', { url: true }), { url: true }), encoding.hexDecode('6869')]",
    )
    .await;
    assert_eq!(
        report.output,
        Some(json!([
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            "qZk+NkcGgWq6PiVxeFDCbJzQ2J0=",
            "7/zfauXrL6LSdBbV8YTfnCWafHk=",
            true,
            false,
            36,
            "aMOpbGxv",
            "héllo",
            "hi",
        ])),
        "{report:?}"
    );
}

#[tokio::test]
async fn helper_errors_are_catchable_errors() {
    let report = run(
        "export default () => [() => crypto.digest('md5', 'x'), () => encoding.base64Decode('//4='), () => new URL('nope'), () => new URL('https://a.example/').host = 'b'].map((f) => { try { f(); return 'no error' } catch (e) { return [e instanceof Error, e.message.split(':')[0]] } })",
    )
    .await;
    assert_eq!(
        report.output,
        Some(json!([
            [true, "unknown algorithm \"md5\"; use \"sha256\" or \"sha1\""],
            [true, "the decoded bytes are not UTF-8 text; scripts work on text only"],
            [true, "invalid URL"],
            [true, "no setter for property"],
        ])),
        "{report:?}"
    );
}

#[tokio::test]
async fn url_and_search_params() {
    let report = run(
        "export default () => { const u = new URL('/hooks/1?a=1&b=2#x', 'https://Example.com:8443'); u.searchParams.set('a', 'one two'); u.searchParams.append('c', '&'); u.searchParams.delete('b'); u.hash = ''; const p = new URLSearchParams({ z: 1, y: 'x' }); p.sort(); const fromPairs = new URLSearchParams([['k', 'v'], ['k', 'w']]); return [u.href, u.host, u.origin, u.pathname, u.search, String(p), fromPairs.getAll('k'), [...new URLSearchParams('?q=a+b&r')], URL.canParse('nope'), JSON.stringify({ u })] }",
    )
    .await;
    assert_eq!(
        report.output,
        Some(json!([
            "https://example.com:8443/hooks/1?a=one+two&c=%26",
            "example.com:8443",
            "https://example.com:8443",
            "/hooks/1",
            "?a=one+two&c=%26",
            "y=x&z=1",
            ["v", "w"],
            [["q", "a b"], ["r", ""]],
            false,
            "{\"u\":\"https://example.com:8443/hooks/1?a=one+two&c=%26\"}",
        ])),
        "{report:?}"
    );
}

#[tokio::test]
async fn search_params_stay_live_after_the_query_changes() {
    let report = run(
        "export default () => { const u = new URL('https://a.example/?x=1'); const p = u.searchParams; u.search = '?y=2'; p.append('z', '3'); const same = u.searchParams === p; u.href = 'https://b.example/?k=v'; return [u.href, same, p.get('k')] }",
    )
    .await;
    assert_eq!(report.output, Some(json!(["https://b.example/?k=v", true, "v"])), "{report:?}");
    let appended = run(
        "export default () => { const u = new URL('https://a.example/?x=1'); const p = u.searchParams; u.search = '?y=2'; p.append('z', '3'); return u.href }",
    )
    .await;
    assert_eq!(appended.output, Some(json!("https://a.example/?y=2&z=3")), "{appended:?}");
}

#[tokio::test]
async fn globals_cannot_be_replaced() {
    let report = run(
        "export default () => { 'use strict'; const out = []; for (const f of [() => { globalThis.crypto = {} }, () => { crypto.hmac = null }, () => { URL.prototype.toString = null }]) { try { f(); out.push('wrote') } catch { out.push('threw') } } return out }",
    )
    .await;
    assert_eq!(report.output, Some(json!(["threw", "threw", "threw"])), "{report:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_reference_entry_exists() {
    use crate::api::Capability;
    let names: Vec<String> =
        crate::reference::entries(&[Capability::Log, Capability::Secrets, Capability::Fetch])
            .into_iter()
            .map(|entry| entry.name)
            .collect();
    let mut req = request(&format!(
        "export default (input, ctx) => {}.filter((name) => name.split('.').reduce((at, key) => at?.[key], {{ ...globalThis, crypto, encoding, URL, URLSearchParams, ctx }}) === undefined)",
        json!(names)
    ));
    req.secrets = Some(BTreeMap::new());
    req.fetch = Some(FetchBridge::new(
        Arc::new(FakeHttp::default()),
        tokio::runtime::Handle::current(),
        Instant::now() + req.budget,
    ));
    let report = pool(1, LIMITS).run(req).await;
    assert_eq!(report.output, Some(json!([])), "missing from a run: {report:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn secrets_and_fetch_when_granted() {
    let http = FakeHttp::default();
    http.respond(Method::POST, "https://hooks.example.com/", StatusCode::OK, r#"{"ok":true}"#);
    let mut req = request(
        "export default async (input, ctx) => { const res = await ctx.fetch('https://hooks.example.com/x', { method: 'post', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ who: input.name, key: ctx.secrets.KEY }) }); return [res.status, res.ok, await res.json()] }",
    );
    req.budget = Duration::from_secs(2);
    req.hard = Duration::from_secs(3);
    req.secrets = Some(BTreeMap::from([("KEY".to_owned(), "s3cret".to_owned())]));
    let deadline = Instant::now() + req.budget;
    req.fetch =
        Some(FetchBridge::new(Arc::new(http.clone()), tokio::runtime::Handle::current(), deadline));
    let report = pool(1, LIMITS).run(req).await;
    assert_eq!(report.output, Some(json!([200, true, { "ok": true }])), "{report:?}");
    let requests = http.requests();
    assert_eq!(requests.len(), 1, "one request");
    assert_eq!(requests[0].0, Method::POST, "method");
    assert_eq!(requests[0].2, r#"{"who":"Ada","key":"s3cret"}"#, "body");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fetch_refuses_private_addresses_and_counts_calls() {
    let http = FakeHttp::default();
    http.respond(Method::GET, "https://example.com/", StatusCode::OK, "hi");
    let mut req = request(
        "export default async (input, ctx) => { const out = []; for (const url of ['http://127.0.0.1/', 'http://localhost:8080/', 'file:///etc/passwd', 'https://example.com/', 'https://example.com/', 'https://example.com/', 'https://example.com/']) { try { await ctx.fetch(url); out.push('ok') } catch (e) { out.push(e.message) } } return out }",
    );
    req.budget = Duration::from_secs(2);
    req.hard = Duration::from_secs(3);
    req.fetch = Some(FetchBridge::new(
        Arc::new(http.clone()),
        tokio::runtime::Handle::current(),
        Instant::now() + req.budget,
    ));
    let report = pool(1, LIMITS).run(req).await;
    assert_eq!(
        report.output,
        Some(json!([
            "127.0.0.1 is not a public address",
            "localhost is not a public address",
            "only http and https URLs can be fetched, not file",
            "ok",
            "ok",
            "a run can fetch at most 5 times",
            "a run can fetch at most 5 times",
        ])),
        "{report:?}"
    );
    assert_eq!(http.requests().len(), 2, "blocked fetches never reached the client");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fetch_encodes_json_and_form_bodies_and_caps_responses() {
    let http = FakeHttp::default();
    http.respond(Method::POST, "https://api.example.com/json", StatusCode::CREATED, "{}");
    http.respond(Method::POST, "https://api.example.com/form", StatusCode::OK, "");
    http.respond(
        Method::GET,
        "https://api.example.com/big",
        StatusCode::OK,
        "x".repeat(MAX_RESPONSE_BYTES + 1),
    );
    let mut req = request(
        "export default async (input, ctx) => { const out = []; out.push((await ctx.fetch('https://api.example.com/json', { method: 'POST', json: { who: input.name } })).status); out.push((await ctx.fetch('https://api.example.com/form', { method: 'POST', headers: { 'Content-Type': 'text/x-custom' }, form: { a: '1 2' } })).ok); for (const init of [{ body: 'x', json: {} }, { body: { a: 1 } }, { headers: { a: 1 } }, { timeoutMs: 0 }, {}]) { try { await ctx.fetch('https://api.example.com/big', init); out.push('ok') } catch (e) { out.push(e.message) } } return out }",
    );
    req.budget = Duration::from_secs(2);
    req.hard = Duration::from_secs(3);
    req.fetch = Some(FetchBridge::new(
        Arc::new(http.clone()),
        tokio::runtime::Handle::current(),
        Instant::now() + req.budget,
    ));
    let report = pool(1, LIMITS).run(req).await;
    assert_eq!(
        report.output,
        Some(json!([
            201,
            true,
            "fetch takes one of body, json and form",
            "fetch body must be a string; use json for objects",
            "fetch header a must be a string",
            "GET api.example.com: timeoutMs must be above 0",
            "GET api.example.com: the response body is over 256 KiB",
        ])),
        "{report:?}"
    );
    let requests = http.requests();
    assert_eq!(requests[0].2, r#"{"who":"Ada"}"#, "json body");
    assert_eq!(requests[1].2, "a=1+2", "form body");
    let headers = http.request_headers();
    assert_eq!(headers[0]["content-type"], "application/json", "json content type");
    assert_eq!(headers[1]["content-type"], "text/x-custom", "a given content type wins");
}

#[tokio::test]
#[ignore = "measures the overrun; run with --ignored --nocapture"]
async fn measure_native_overrun() {
    let pool = big_pool(1);
    let mut req = request(LONG_SORT);
    req.budget = Duration::from_secs(60);
    req.hard = Duration::from_secs(60);
    let report = pool.run(req).await;
    tracing::info!(?report.duration, "native sort of 1e6 values");
    assert_eq!(report.status, RunStatus::Ok, "{report:?}");
}
