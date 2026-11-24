//! The load generator: open-loop beacon sends at a fixed rate, with
//! latency measured from each send's intended time, so a stalled server
//! shows up as latency instead of as a slower send rate.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::Context as _;
use reqwest::StatusCode;
use tokio::sync::Semaphore;
use tokio::time::Instant;

use crate::report::{Labels, Percentiles, RunResult};
use crate::serve::{ServerStats, Timed};

/// `load` flags.
#[derive(Debug, Clone, clap::Args)]
pub struct LoadArgs {
    /// The server's base URL.
    #[arg(long, default_value = "http://127.0.0.1:8600")]
    pub url: String,
    /// Events per second across all projects.
    #[arg(long)]
    pub rate: u32,
    /// How many projects the events spread over.
    #[arg(long, default_value_t = 10)]
    pub projects: u32,
    /// Seconds of measured load.
    #[arg(long, default_value_t = 60)]
    pub duration: u64,
    /// The hot project for bursts, queries and rollups.
    #[arg(long, default_value_t = 0)]
    pub hot: u32,
    /// During the burst the offered load is this many times `--rate`; the
    /// extra all goes to the hot project.
    #[arg(long, default_value_t = 1)]
    pub burst_x: u32,
    /// Seconds into the run the burst starts.
    #[arg(long, default_value_t = 15)]
    pub burst_at: u64,
    /// Seconds the burst lasts.
    #[arg(long, default_value_t = 30)]
    pub burst_for: u64,
    /// Dashboard queries per second on the hot project.
    #[arg(long, default_value_t = 0)]
    pub queries_per_s: u32,
    /// App sign-in requests per second.
    #[arg(long, default_value_t = 5)]
    pub app_per_s: u32,
    /// Seconds between rollups of the hot project; 0 for none.
    #[arg(long, default_value_t = 0)]
    pub rollup_every: u64,
    /// Requests allowed in flight before sends count as client errors.
    #[arg(long, default_value_t = 4096)]
    pub max_inflight: usize,
    /// The first seq ID; keep warmup and measured runs apart.
    #[arg(long, default_value_t = 1)]
    pub seq_start: u64,
    /// Where the run's JSON goes; acked seq IDs go next to it as `.acked`.
    #[arg(long)]
    pub out: PathBuf,
    #[command(flatten)]
    pub labels: Labels,
}

/// The browsers the beacons claim to come from.
const AGENTS: [&str; 4] = [
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:144.0) Gecko/20100101 Firefox/144.0",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
];
const PATHS: [&str; 6] = ["/", "/pricing", "/features", "/blog/launch", "/signup", "/docs/start"];
const REFERRERS: [&str; 4] = ["", "https://www.google.com/", "https://news.ycombinator.com/", ""];

/// How many sends are due `elapsed` into a run at `rate` per second.
#[must_use]
pub fn due(elapsed: Duration, rate: u32) -> u64 {
    let nanos = elapsed.as_nanos().saturating_mul(u128::from(rate)) / 1_000_000_000;
    u64::try_from(nanos).unwrap_or(u64::MAX)
}

/// The intended time of send `n` at `rate` per second.
#[must_use]
pub fn intended(start: Instant, n: u64, rate: u32) -> Instant {
    let nanos = u128::from(n) * 1_000_000_000 / u128::from(rate.max(1));
    start + Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

/// The beacon body for event `seq`, as the snippet sends it.
#[must_use]
pub fn beacon(seq: u64) -> String {
    let pick = |n: usize| usize::try_from(seq).unwrap_or(0) % n;
    let path = PATHS.get(pick(PATHS.len())).copied().unwrap_or("/");
    let referrer = REFERRERS.get(pick(REFERRERS.len())).copied().unwrap_or("");
    let (name, extra) = if seq.is_multiple_of(5) {
        ("signup_started", r#","p":{"plan":"pro","seq":SEQ}"#)
    } else {
        ("pageview", r#","p":{"seq":SEQ}"#)
    };
    let referrer =
        if referrer.is_empty() { String::new() } else { format!(r#","r":"{referrer}""#) };
    let extra = extra.replace("SEQ", &seq.to_string());
    format!(
        r#"{{"s":"bench","n":"{name}","u":"https://site.test{path}?utm_source=x","w":1440{referrer}{extra}}}"#
    )
}

#[derive(Debug, Default)]
struct Tally {
    sent: AtomicU64,
    acked: AtomicU64,
    rejected: AtomicU64,
    errors: AtomicU64,
    latencies: Mutex<Vec<f64>>,
    acked_seqs: Mutex<Vec<u64>>,
}

impl Tally {
    /// Waits until every sent request has an answer, or `limit` passes.
    async fn drain(&self, limit: Duration) {
        let deadline = Instant::now() + limit;
        while self.acked.load(Ordering::Relaxed)
            + self.rejected.load(Ordering::Relaxed)
            + self.errors.load(Ordering::Relaxed)
            < self.sent.load(Ordering::Relaxed)
            && Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Writes the acked seq IDs, one per line.
    fn write_acked(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let seqs =
            std::mem::take(&mut *self.acked_seqs.lock().unwrap_or_else(PoisonError::into_inner));
        let text = seqs.iter().map(u64::to_string).collect::<Vec<_>>().join("\n");
        std::fs::write(path, text).context("writing acked IDs")
    }

    fn record(&self, seq: u64, status: Option<StatusCode>, ms: f64) {
        match status {
            Some(StatusCode::ACCEPTED) => {
                self.acked.fetch_add(1, Ordering::Relaxed);
                self.acked_seqs.lock().unwrap_or_else(PoisonError::into_inner).push(seq);
                self.latencies.lock().unwrap_or_else(PoisonError::into_inner).push(ms);
            }
            Some(StatusCode::SERVICE_UNAVAILABLE) => {
                self.rejected.fetch_add(1, Ordering::Relaxed);
            }
            Some(_) | None => {
                self.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Sender {
    client: reqwest::Client,
    url: String,
    tally: Arc<Tally>,
    seq: Arc<AtomicU64>,
    inflight: Arc<Semaphore>,
}

impl Sender {
    /// Sends one beacon to `project`, timed from `intended`.
    fn send(&self, project: u32, intended: Instant) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        self.tally.sent.fetch_add(1, Ordering::Relaxed);
        let Ok(permit) = Arc::clone(&self.inflight).try_acquire_owned() else {
            self.tally.errors.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let this = self.clone();
        tokio::spawn(async move {
            let pick = usize::try_from(seq).unwrap_or(0);
            let agent = AGENTS.get(pick % AGENTS.len()).copied().unwrap_or("");
            let ip = format!("198.51.{}.{}", (seq / 250) % 250, seq % 250);
            let response = this
                .client
                .post(format!("{}/e/{project}", this.url))
                .header("user-agent", agent)
                .header("x-forwarded-for", ip)
                .body(beacon(seq))
                .send()
                .await;
            let ms = intended.elapsed().as_secs_f64() * 1000.0;
            this.tally.record(seq, response.ok().map(|r| r.status()), ms);
            drop(permit);
        });
    }
}

/// Runs open-loop sends at `rate` to the projects `pick` names until `end`.
async fn stream(
    sender: Sender,
    rate: u32,
    start: Instant,
    end: Instant,
    pick: impl Fn(u64) -> u32,
) {
    if rate == 0 {
        return;
    }
    let mut n = 0_u64;
    loop {
        let now = Instant::now();
        if now >= end {
            break;
        }
        let target = due(now - start, rate);
        while n < target {
            sender.send(pick(n), intended(start, n, rate));
            n += 1;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

/// A closed-loop probe at `per_s`, returning each request's milliseconds:
/// round trip for the app, the server's own time for timed probes.
async fn probe(
    client: reqwest::Client,
    url: String,
    per_s: u32,
    end: Instant,
    server_timed: bool,
) -> Vec<f64> {
    let mut out = Vec::new();
    if per_s == 0 {
        return out;
    }
    let mut tick = tokio::time::interval(Duration::from_secs_f64(1.0 / f64::from(per_s)));
    while Instant::now() < end {
        tick.tick().await;
        let started = Instant::now();
        let request = if server_timed { client.get(&url) } else { client.post(&url) };
        let Ok(response) = request.send().await else { continue };
        let ms = if server_timed {
            match response.json::<Timed>().await {
                Ok(timed) => timed.ms,
                Err(_) => continue,
            }
        } else {
            started.elapsed().as_secs_f64() * 1000.0
        };
        out.push(ms);
    }
    out
}

/// Rollups of the hot project every `every` seconds; server-side ms each.
async fn rollups(client: reqwest::Client, url: String, every: u64, end: Instant) -> Vec<f64> {
    let mut out = Vec::new();
    if every == 0 {
        return out;
    }
    loop {
        tokio::time::sleep(Duration::from_secs(every)).await;
        if Instant::now() >= end {
            break;
        }
        if let Ok(response) = client.post(&url).send().await
            && let Ok(timed) = response.json::<Timed>().await
        {
            out.push(timed.ms);
        }
    }
    out
}

async fn server_stats(client: &reqwest::Client, url: &str) -> anyhow::Result<ServerStats> {
    let response =
        client.get(format!("{url}/stats")).send().await.context("reading server stats")?;
    Ok(response.json().await?)
}

/// Runs the load, waits for in-flight requests, and returns the result with
/// the server's stats. Writes the acked seq IDs to `<out>.acked`.
///
/// # Errors
///
/// When the acked file can't be written.
pub async fn run(args: &LoadArgs) -> anyhow::Result<RunResult> {
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(args.max_inflight)
        .timeout(Duration::from_secs(30))
        .build()?;
    let tally = Arc::new(Tally::default());
    let sender = Sender {
        client: client.clone(),
        url: args.url.clone(),
        tally: Arc::clone(&tally),
        seq: Arc::new(AtomicU64::new(args.seq_start)),
        inflight: Arc::new(Semaphore::new(args.max_inflight)),
    };
    let start = Instant::now();
    let end = start + Duration::from_secs(args.duration);
    let projects = args.projects.max(1);
    let base = tokio::spawn(stream(sender.clone(), args.rate, start, end, move |n| {
        u32::try_from(n % u64::from(projects)).unwrap_or(0)
    }));
    let hot = args.hot;
    let burst_rate = args.rate.saturating_mul(args.burst_x.saturating_sub(1));
    let burst_start = start + Duration::from_secs(args.burst_at);
    let burst_end = (burst_start + Duration::from_secs(args.burst_for)).min(end);
    let burst = tokio::spawn(async move {
        tokio::time::sleep_until(burst_start).await;
        stream(sender, burst_rate, burst_start, burst_end, move |_| hot).await;
    });
    let queries = tokio::spawn(probe(
        client.clone(),
        format!("{}/q/{hot}", args.url),
        args.queries_per_s,
        end,
        true,
    ));
    let app = tokio::spawn(probe(
        client.clone(),
        format!("{}/app", args.url),
        args.app_per_s,
        end,
        false,
    ));
    let rolled = tokio::spawn(rollups(
        client.clone(),
        format!("{}/rollup/{hot}", args.url),
        args.rollup_every,
        end,
    ));
    base.await?;
    burst.await?;
    let query_ms = queries.await?;
    let app_ms = app.await?;
    let rollup_ms = rolled.await?;
    tally.drain(Duration::from_secs(30)).await;
    // After a kill -9 there is no server to ask; the run still gets written.
    let server = server_stats(&client, &args.url).await.unwrap_or_default();
    tally.write_acked(&args.out.with_extension("acked"))?;
    let mut latencies =
        std::mem::take(&mut *tally.latencies.lock().unwrap_or_else(PoisonError::into_inner));
    Ok(RunResult {
        labels: args.labels.clone(),
        rate: args.rate,
        projects,
        duration_s: args.duration,
        sent: tally.sent.load(Ordering::Relaxed),
        acked: tally.acked.load(Ordering::Relaxed),
        rejected: tally.rejected.load(Ordering::Relaxed),
        errors: tally.errors.load(Ordering::Relaxed),
        ingest_ms: Percentiles::of(&mut latencies),
        query_ms: (!query_ms.is_empty()).then(|| Percentiles::of(&mut query_ms.clone())),
        app_ms: Percentiles::of(&mut app_ms.clone()),
        rollup_ms,
        cpu_pct: None,
        rss_mb: None,
        server,
        lost: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_are_due_at_the_rate() {
        assert_eq!(due(Duration::from_secs(1), 2000), 2000);
        assert_eq!(due(Duration::from_millis(1), 2000), 2);
        assert_eq!(due(Duration::from_micros(400), 2000), 0);
        let start = Instant::now();
        assert_eq!(intended(start, 1000, 2000) - start, Duration::from_millis(500));
    }

    #[test]
    fn beacons_parse_and_carry_their_seq() {
        for seq in [1, 5, 12] {
            let body = beacon(seq);
            let parsed = bizkit_analytics::beacon::parse(body.as_bytes()).unwrap();
            assert_eq!(parsed.props.unwrap()["seq"], seq, "{body}");
        }
    }
}
