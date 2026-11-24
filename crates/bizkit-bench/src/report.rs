//! Run results: their JSON shape, the `finish` step that adds container
//! stats and crash loss, and the markdown tables.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::pipeline::Design;
use crate::serve::ServerStats;
use crate::store::{Layout, Sync};

/// What a run was, recorded with its numbers.
#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct Labels {
    /// The scenario name, such as `steady` or `kill`.
    #[arg(long, default_value = "adhoc")]
    pub scenario: String,
    /// The server's design.
    #[arg(long = "label-design", value_enum)]
    pub design: Design,
    /// The server's layout.
    #[arg(long = "label-layout", value_enum, default_value = "per-project")]
    pub layout: Layout,
    /// The server's synchronous mode.
    #[arg(long = "label-sync", value_enum, default_value = "normal")]
    pub sync: Sync,
    /// The server's flush interval, for buffered and hybrid.
    #[arg(long = "label-flush-ms", default_value_t = 250)]
    pub flush_ms: u64,
}

/// Latency or resource percentiles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Percentiles {
    /// Samples.
    pub n: usize,
    /// Median.
    pub p50: f64,
    /// 95th percentile.
    pub p95: f64,
    /// 99th percentile.
    pub p99: f64,
    /// Largest sample.
    pub max: f64,
}

impl Percentiles {
    /// Percentiles of `samples` (sorted in place), nearest-rank.
    #[must_use]
    pub fn of(samples: &mut [f64]) -> Self {
        samples.sort_by(f64::total_cmp);
        let at = |per_mille: usize| -> f64 {
            let rank = (per_mille * samples.len()).div_ceil(1000).max(1);
            samples.get(rank - 1).copied().unwrap_or(0.0)
        };
        Self { n: samples.len(), p50: at(500), p95: at(950), p99: at(990), max: at(1000) }
    }
}

/// One run's numbers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    /// What ran.
    pub labels: Labels,
    /// Offered events per second.
    pub rate: u32,
    /// Projects the events spread over.
    pub projects: u32,
    /// Seconds of load.
    pub duration_s: u64,
    /// Beacons sent.
    pub sent: u64,
    /// Beacons answered 202.
    pub acked: u64,
    /// Beacons answered 503 (queue full).
    pub rejected: u64,
    /// Other failures, including sends over the in-flight cap.
    pub errors: u64,
    /// Ingest latency from intended send time, acked beacons only.
    pub ingest_ms: Percentiles,
    /// Dashboard query time on the server, when queries ran.
    pub query_ms: Option<Percentiles>,
    /// App sign-in round trip.
    pub app_ms: Percentiles,
    /// Rollup times on the server.
    pub rollup_ms: Vec<f64>,
    /// Container CPU, percent of one core, from 1 s samples.
    pub cpu_pct: Option<Percentiles>,
    /// Container memory in MB, from 1 s samples.
    pub rss_mb: Option<Percentiles>,
    /// The server's stats at the end of the run.
    pub server: ServerStats,
    /// Acked beacons missing after a kill -9 and restart.
    pub lost: Option<u64>,
}

impl RunResult {
    /// Acked events per second over the run.
    #[must_use]
    pub fn throughput(&self) -> f64 {
        let acked = u32::try_from(self.acked).map_or(f64::MAX, f64::from);
        let secs = u32::try_from(self.duration_s.max(1)).map_or(f64::MAX, f64::from);
        acked / secs
    }
}

/// `finish` flags.
#[derive(Debug, Clone, clap::Args)]
pub struct FinishArgs {
    /// The run's JSON, updated in place.
    #[arg(long)]
    pub run: PathBuf,
    /// `docker stats` samples, one `<cpu%>,<mem>` line per second.
    #[arg(long)]
    pub docker_stats: Option<PathBuf>,
    /// The data dir to count stored events in, after a kill.
    #[arg(long)]
    pub data: Option<PathBuf>,
}

/// Adds container stats and, with `--data`, the acked-but-lost count.
///
/// # Errors
///
/// When a file can't be read or written.
pub async fn finish(args: &FinishArgs) -> anyhow::Result<RunResult> {
    let text = std::fs::read_to_string(&args.run)
        .with_context(|| format!("reading {}", args.run.display()))?;
    let mut run: RunResult = serde_json::from_str(&text)?;
    if let Some(path) = &args.docker_stats {
        let samples =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let (mut cpu, mut mem) = docker_samples(&samples);
        run.cpu_pct = Some(Percentiles::of(&mut cpu));
        run.rss_mb = Some(Percentiles::of(&mut mem));
    }
    if let Some(data) = &args.data {
        let acked_path = args.run.with_extension("acked");
        let acked = std::fs::read_to_string(&acked_path)
            .with_context(|| format!("reading {}", acked_path.display()))?;
        let stored: HashSet<u64> = crate::store::stored_seqs(data).await?.into_iter().collect();
        run.lost = Some(lost(&acked, &stored));
    }
    std::fs::write(&args.run, serde_json::to_string_pretty(&run)?)?;
    Ok(run)
}

/// Acked seq IDs (one per line) that `stored` lacks.
#[must_use]
pub fn lost<S: std::hash::BuildHasher>(acked: &str, stored: &HashSet<u64, S>) -> u64 {
    let missing = acked
        .lines()
        .filter_map(|line| line.trim().parse::<u64>().ok())
        .filter(|seq| !stored.contains(seq))
        .count();
    u64::try_from(missing).unwrap_or(u64::MAX)
}

/// Parses `docker stats --format '{{.CPUPerc}},{{.MemUsage}}'` lines into
/// CPU percent and memory MB.
#[must_use]
pub fn docker_samples(text: &str) -> (Vec<f64>, Vec<f64>) {
    let mut cpu = Vec::new();
    let mut mem = Vec::new();
    for line in text.lines() {
        let Some((cpu_text, mem_text)) = line.split_once(',') else { continue };
        let used = mem_text.split('/').next().unwrap_or("").trim();
        if let (Ok(c), Some(m)) =
            (cpu_text.trim().trim_end_matches('%').parse::<f64>(), megabytes(used))
        {
            cpu.push(c);
            mem.push(m);
        }
    }
    (cpu, mem)
}

fn megabytes(text: &str) -> Option<f64> {
    let units = [("GiB", 1024.0), ("MiB", 1.0), ("KiB", 1.0 / 1024.0), ("B", 1.0 / 1_048_576.0)];
    units.iter().find_map(|(suffix, scale)| {
        text.strip_suffix(suffix).and_then(|n| n.trim().parse::<f64>().ok()).map(|n| n * scale)
    })
}

/// Reads every run JSON under `dir`.
///
/// # Errors
///
/// When the directory or a file can't be read.
pub fn load_runs(dir: &Path) -> anyhow::Result<Vec<RunResult>> {
    let mut runs = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let text = std::fs::read_to_string(&path)?;
            runs.push(
                serde_json::from_str(&text)
                    .with_context(|| format!("parsing {}", path.display()))?,
            );
        }
    }
    runs.sort_by(|a: &RunResult, b| {
        let key =
            |r: &RunResult| (r.projects, r.rate, design_order(r.labels.design), r.labels.flush_ms);
        a.labels.scenario.cmp(&b.labels.scenario).then_with(|| key(a).cmp(&key(b)))
    });
    Ok(runs)
}

const fn design_order(design: Design) -> u8 {
    match design {
        Design::WriteThrough => 0,
        Design::Batched => 1,
        Design::Buffered => 2,
        Design::Hybrid => 3,
    }
}

fn design_name(run: &RunResult) -> String {
    let mut name = match run.labels.design {
        Design::WriteThrough => "write-through".to_owned(),
        Design::Batched => "batched".to_owned(),
        Design::Buffered => format!("buffered {}ms", run.labels.flush_ms),
        Design::Hybrid => format!("hybrid {}ms", run.labels.flush_ms),
    };
    if run.labels.layout == Layout::Shared {
        name.push_str(", shared");
    }
    if run.labels.sync == Sync::Full {
        name.push_str(", sync=full");
    }
    name
}

/// Markdown tables, one per scenario.
#[must_use]
pub fn markdown(runs: &[RunResult]) -> String {
    let mut out = String::new();
    let mut scenario = "";
    for run in runs {
        if run.labels.scenario != scenario {
            scenario = &run.labels.scenario;
            write!(
                out,
                "\n### {scenario}\n\n| design | offered/s | projects | acked/s | p50 ms | p95 ms | p99 ms | 503s | errors | CPU p95 % | RSS p95 MB | app p99 ms | query p99 ms | rollup max ms | WAL MB | lost |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n"
            )
            .unwrap_or_default();
        }
        let opt = |p: Option<Percentiles>, f: fn(&Percentiles) -> f64| {
            p.map_or_else(|| "-".to_owned(), |p| format!("{:.1}", f(&p)))
        };
        let rollup = run
            .rollup_ms
            .iter()
            .copied()
            .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
        let wal = u32::try_from(run.server.wal_bytes / 1024).map_or(f64::MAX, f64::from) / 1024.0;
        writeln!(
            out,
            "| {} | {} | {} | {:.0} | {:.1} | {:.1} | {:.1} | {} | {} | {} | {} | {:.1} | {} | {} | {:.1} | {} |",
            design_name(run),
            run.rate,
            run.projects,
            run.throughput(),
            run.ingest_ms.p50,
            run.ingest_ms.p95,
            run.ingest_ms.p99,
            run.rejected,
            run.errors,
            opt(run.cpu_pct, |p| p.p95),
            opt(run.rss_mb, |p| p.p95),
            run.app_ms.p99,
            opt(run.query_ms, |p| p.p99),
            rollup.map_or_else(|| "-".to_owned(), |ms| format!("{ms:.0}")),
            wal,
            run.lost.map_or_else(|| "-".to_owned(), |n| n.to_string()),
        )
        .unwrap_or_default();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles() {
        let mut samples: Vec<f64> = (1..=100).rev().map(f64::from).collect();
        let p = Percentiles::of(&mut samples);
        assert_eq!((p.n, p.p50, p.p95, p.p99, p.max), (100, 50.0, 95.0, 99.0, 100.0), "{p:?}");
        assert_eq!(Percentiles::of(&mut []).p99, 0.0, "empty");
    }

    #[test]
    fn lost_counts_acked_ids_not_stored() {
        let stored: HashSet<u64> = [1, 2, 4].into_iter().collect();
        assert_eq!(lost("1\n2\n3\n4\n5\n", &stored), 2);
    }

    #[test]
    fn docker_stats_lines_parse() {
        let (cpu, mem) = docker_samples("153.20%,312.5MiB / 1GiB\n0.50%,1.2GiB / 1GiB\njunk\n");
        assert_eq!(cpu, [153.2, 0.5]);
        assert!((mem[1] - 1228.8).abs() < 0.01, "{mem:?}");
    }
}
