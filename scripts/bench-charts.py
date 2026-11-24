#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["matplotlib>=3.9,<3.11"]
# ///
"""Renders the analytics ingest benchmark charts from a results dir.

    scripts/bench-charts.py <results-dir> <out-dir> [prefix]

Writes <prefix>-{throughput,p99,latency-vs-load,resources,kill-loss,windows}.png.
"""

import json
import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt

DESIGNS = ["write-through", "batched", "buffered", "hybrid"]
# The first four slots of the dataviz reference palette, in its order: they
# pass the color-vision checks for adjacent bars. All four do not pass as
# lines, where every pair must differ, so the line chart leaves batched out.
COLOR = {
    "write-through": "#2a78d6",
    "batched": "#eb6834",
    "buffered": "#1baf7a",
    "hybrid": "#eda100",
}
WINDOWED = ["buffered", "hybrid"]
HATCH_100MS = "///"
SURFACE = "#fcfcfb"
INK = "#0b0b0b"
INK_2 = "#52514e"
GRID = "#e4e3df"
CAP_MB = 1024

plt.rcParams.update(
    {
        "figure.facecolor": SURFACE,
        "axes.facecolor": SURFACE,
        "axes.edgecolor": GRID,
        "axes.labelcolor": INK_2,
        "axes.titlecolor": INK,
        "axes.titlesize": 13,
        "axes.titleweight": "bold",
        "axes.grid": True,
        "axes.axisbelow": True,
        "grid.color": GRID,
        "grid.linewidth": 0.8,
        "xtick.color": INK_2,
        "ytick.color": INK_2,
        "axes.spines.top": False,
        "axes.spines.right": False,
        "legend.frameon": False,
        "font.size": 10,
        "savefig.dpi": 200,
    }
)


def load(results: Path) -> list[dict]:
    runs = []
    for f in sorted(results.glob("*.json")):
        r = json.loads(f.read_text())
        if "labels" in r:
            runs.append(r)
    return runs


def lab(r: dict, key: str):
    return r["labels"][key]


def default_case(r: dict) -> bool:
    return lab(r, "sync") == "normal" and lab(r, "flush_ms") == 250


def scenario_runs(runs: list[dict]) -> list[tuple[str, dict[str, dict]]]:
    """Groups the non-kill comparison runs into (label, {design: run})."""
    groups: dict[str, dict[str, dict]] = {}
    order: list[str] = []

    def add(label: str, r: dict):
        if label not in groups:
            groups[label] = {}
            order.append(label)
        groups[label][lab(r, "design")] = r

    def key(r):
        return (r["projects"] != 10, r["projects"], r["rate"])

    steady = [r for r in runs if lab(r, "scenario") == "steady" and default_case(r)]
    for r in sorted(steady, key=key):
        add(f"steady\n{r['rate']:,}/s\n{r['projects']} proj", r)
    for name in ["burst", "mixed"]:
        for r in runs:
            if lab(r, "scenario") == name and default_case(r):
                suffix = " +10x hot" if name == "burst" else ""
                add(f"{name}\n{r['rate']:,}/s{suffix}\n{r['projects']} proj", r)
    shared = [r for r in runs if lab(r, "scenario") == "shared"]
    for r in sorted(shared, key=key):
        add(f"shared DB\n{r['rate']:,}/s\n{r['projects']} proj", r)
    return [(k, groups[k]) for k in order]


def throughput(r: dict) -> float:
    return r["acked"] / r["duration_s"]


def grouped_bars(ax, groups, value, log=False):
    n = len(DESIGNS)
    width = 0.8 / n
    for i, d in enumerate(DESIGNS):
        xs, ys = [], []
        for gi, (_, by) in enumerate(groups):
            if d in by:
                xs.append(gi - 0.4 + width * (i + 0.5))
                ys.append(value(by[d]))
        ax.bar(
            xs,
            ys,
            width * 0.92,
            color=COLOR[d],
            label=d,
            edgecolor=SURFACE,
            linewidth=1,
        )
    ax.set_xticks(range(len(groups)))
    ax.set_xticklabels([g for g, _ in groups], fontsize=8)
    ax.grid(axis="x", visible=False)
    if log:
        ax.set_yscale("log")


def save(fig, out: Path):
    fig.tight_layout()
    fig.savefig(out, facecolor=SURFACE)
    plt.close(fig)
    print(out)


def chart_throughput(runs, out):
    groups = scenario_runs(runs)
    fig, ax = plt.subplots(figsize=(14, 5))
    grouped_bars(ax, groups, throughput)
    offered = [max(by.values(), key=lambda r: r["rate"]) for _, by in groups]
    for gi, r in enumerate(offered):
        if lab(r, "scenario") != "burst":
            ax.hlines(
                r["rate"],
                gi - 0.42,
                gi + 0.42,
                colors=INK_2,
                linestyles=":",
                linewidth=1,
            )
    ax.set_ylabel("acked events/s (dotted: offered)")
    ax.set_title("Sustained throughput per scenario (250 ms flush)")
    ax.legend(loc="upper left", ncols=4)
    save(fig, out)


def chart_p99(runs, out):
    groups = scenario_runs(runs)
    fig, ax = plt.subplots(figsize=(14, 5))
    grouped_bars(ax, groups, lambda r: max(r["ingest_ms"]["p99"], 0.01), log=True)
    ax.set_ylabel("p99 ingest latency (ms, log)")
    ax.set_title("p99 ingest latency per scenario (250 ms flush)")
    ax.legend(loc="upper left", ncols=4)
    save(fig, out)


def chart_latency_vs_load(runs, out):
    fig, ax = plt.subplots(figsize=(9, 5.5))
    for d in (d for d in DESIGNS if d != "batched"):
        rs = sorted(
            (
                r
                for r in runs
                if lab(r, "scenario") == "steady"
                and default_case(r)
                and r["projects"] == 10
                and lab(r, "design") == d
            ),
            key=lambda r: r["rate"],
        )
        if not rs:
            continue
        x = [r["rate"] for r in rs]
        p99 = [r["ingest_ms"]["p99"] for r in rs]
        p50 = [r["ingest_ms"]["p50"] for r in rs]
        ax.plot(
            x,
            p99,
            color=COLOR[d],
            linewidth=2,
            marker="o",
            markersize=7,
            label=f"{d} p99",
        )
        ax.plot(
            x,
            p50,
            color=COLOR[d],
            linewidth=2,
            linestyle="--",
            marker="s",
            markersize=6,
            label=f"{d} p50",
        )
    ax.set_yscale("log")
    ax.set_xlabel("offered load (events/s), 10 projects")
    ax.set_ylabel("ingest latency (ms, log)")
    ax.set_title("Latency vs offered load (solid p99, dashed p50)")
    ax.legend(ncols=3, fontsize=8, loc="upper left")
    save(fig, out)


def chart_resources(runs, out):
    sel = [
        r
        for r in runs
        if lab(r, "scenario") == "steady"
        and default_case(r)
        and r["rate"] == 5000
        and r["projects"] in (10, 100)
    ]
    groups = []
    for p in (10, 100):
        by = {lab(r, "design"): r for r in sel if r["projects"] == p}
        if by:
            groups.append((f"5,000/s\n{p} projects", by))
    fig, (cpu, rss) = plt.subplots(1, 2, figsize=(11, 4.8))
    grouped_bars(cpu, groups, lambda r: r["cpu_pct"]["p95"])
    cpu.axhline(200, color=INK_2, linestyle=":", linewidth=1)
    cpu.text(-0.45, 203, "2-core cap (200%)", color=INK_2, fontsize=8, va="bottom")
    cpu.set_ylim(0, 230)
    cpu.set_ylabel("CPU p95 (% of one core)")
    cpu.set_title("CPU")
    grouped_bars(rss, groups, lambda r: r["rss_mb"]["p95"])
    rss.axhline(CAP_MB, color=INK_2, linestyle=":", linewidth=1)
    rss.text(-0.45, CAP_MB * 1.02, "1 GB cap", color=INK_2, fontsize=8, va="bottom")
    rss.set_ylim(0, CAP_MB * 1.15)
    rss.set_ylabel("RSS p95 (MB)")
    rss.set_title("Memory")
    fig.legend(*cpu.get_legend_handles_labels(), loc="lower center", ncols=4)
    fig.tight_layout(rect=(0, 0.07, 1, 1))
    fig.savefig(out, facecolor=SURFACE)
    plt.close(fig)
    print(out)


def chart_kill_loss(runs, out):
    kills = [r for r in runs if lab(r, "scenario") == "kill"]
    bars = []
    for d in DESIGNS:
        for r in sorted(
            (r for r in kills if lab(r, "design") == d),
            key=lambda r: lab(r, "flush_ms"),
        ):
            name = f"{d}\n{lab(r, 'flush_ms')} ms flush" if d in WINDOWED else d
            bars.append((name, d, r.get("lost") or 0, r["acked"]))
    fig, ax = plt.subplots(figsize=(11, 5))
    xs = range(len(bars))
    ax.bar(
        xs,
        [b[2] for b in bars],
        0.6,
        color=[COLOR[b[1]] for b in bars],
        edgecolor=SURFACE,
    )
    top = max([b[2] for b in bars] + [1])
    for x, (_, _, lost, acked) in zip(xs, bars):
        ax.text(
            x,
            lost + top * 0.02,
            f"{lost:,}\nof {acked:,} acked",
            ha="center",
            va="bottom",
            fontsize=8,
            color=INK,
        )
    ax.set_ylim(0, top * 1.25)
    ax.set_xticks(list(xs))
    ax.set_xticklabels([b[0] for b in bars], fontsize=9)
    ax.grid(axis="x", visible=False)
    ax.set_ylabel("acked events lost (count)")
    ax.set_title("Events lost on kill -9 mid-run (2,000/s, 10 projects)")
    handles = [plt.Rectangle((0, 0), 1, 1, color=COLOR[d]) for d in DESIGNS]
    ax.legend(handles, DESIGNS, loc="upper left")
    save(fig, out)


def chart_windows(runs, out):
    """Buffered and hybrid at each flush window, side by side."""
    cases = [
        ("steady", 2000, 10),
        ("steady", 5000, 10),
        ("steady", 10000, 10),
        ("steady", 5000, 100),
        ("burst", 500, 10),
        ("mixed", 2000, 10),
    ]
    series = [(d, w) for d in WINDOWED for w in (100, 250)]

    def find(scenario, rate, projects, design, window):
        for r in runs:
            if (
                lab(r, "scenario") == scenario
                and r["rate"] == rate
                and r["projects"] == projects
                and lab(r, "design") == design
                and lab(r, "flush_ms") == window
                and lab(r, "sync") == "normal"
            ):
                return r
        return None

    fig, (lat, cpu) = plt.subplots(2, 1, figsize=(12, 8), sharex=True)
    width = 0.8 / len(series)
    for i, (d, w) in enumerate(series):
        xs, p99, cpus = [], [], []
        for ci, case in enumerate(cases):
            r = find(*case, d, w)
            if r:
                xs.append(ci - 0.4 + width * (i + 0.5))
                p99.append(max(r["ingest_ms"]["p99"], 0.01))
                cpus.append(r["cpu_pct"]["p95"])
        style = {
            "color": COLOR[d],
            "edgecolor": SURFACE,
            "linewidth": 1,
            "hatch": HATCH_100MS if w == 100 else None,
            "label": f"{d} {w} ms",
        }
        lat.bar(xs, p99, width * 0.92, **style)
        cpu.bar(xs, cpus, width * 0.92, **style)
    lat.set_yscale("log")
    lat.set_ylabel("p99 ingest latency (ms, log)")
    lat.set_title("Buffered vs hybrid by flush window (hatched: 100 ms)")
    cpu.set_ylabel("CPU p95 (% of one core)")
    for ax in (lat, cpu):
        ax.grid(axis="x", visible=False)
    labels = [
        f"{s}\n{rate:,}/s{' +10x hot' if s == 'burst' else ''}\n{p} proj"
        for s, rate, p in cases
    ]
    cpu.set_xticks(range(len(cases)))
    cpu.set_xticklabels(labels, fontsize=8)
    fig.legend(*lat.get_legend_handles_labels(), loc="lower center", ncols=4)
    fig.tight_layout(rect=(0, 0.05, 1, 1))
    fig.savefig(out, facecolor=SURFACE)
    plt.close(fig)
    print(out)


def main():
    if len(sys.argv) < 3:
        sys.exit("usage: bench-charts.py <results-dir> <out-dir> [prefix]")
    results, out = Path(sys.argv[1]), Path(sys.argv[2])
    prefix = sys.argv[3] if len(sys.argv) > 3 else "analytics-ingest-bench"
    out.mkdir(parents=True, exist_ok=True)
    runs = load(results)
    if not runs:
        sys.exit(f"no run results in {results}")
    chart_throughput(runs, out / f"{prefix}-throughput.png")
    chart_p99(runs, out / f"{prefix}-p99.png")
    chart_latency_vs_load(runs, out / f"{prefix}-latency-vs-load.png")
    chart_resources(runs, out / f"{prefix}-resources.png")
    chart_kill_loss(runs, out / f"{prefix}-kill-loss.png")
    chart_windows(runs, out / f"{prefix}-windows.png")


if __name__ == "__main__":
    main()
