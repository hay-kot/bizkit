//! The `bizkit-bench` binary.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context as _;
use bizkit_bench::{load, report, serve};
use clap::{Parser, Subcommand};

/// Benchmarks analytics ingest pipelines.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve one pipeline design until SIGTERM or SIGINT.
    Serve(serve::ServeArgs),
    /// Drive a server with open-loop load and write the run's JSON.
    Load(load::LoadArgs),
    /// Add container stats and crash loss to a run's JSON.
    Finish(report::FinishArgs),
    /// Print markdown tables for every run in a directory.
    Report {
        /// The directory of run JSON files.
        dir: PathBuf,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let mut stdout = std::io::stdout().lock();
    match run(Cli::parse(), &mut stdout).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            writeln!(std::io::stderr(), "bizkit-bench: {err:#}").unwrap_or_default();
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli, out: &mut impl std::io::Write) -> anyhow::Result<()> {
    match cli.command {
        Command::Serve(args) => {
            let server = serve::start(&args).await?;
            writeln!(out, "serving {:?} on {}", args.design, server.addr)?;
            out.flush()?;
            shutdown_signal().await;
            server.shutdown().await;
        }
        Command::Load(args) => {
            let result = load::run(&args).await?;
            std::fs::write(&args.out, serde_json::to_string_pretty(&result)?)
                .with_context(|| format!("writing {}", args.out.display()))?;
            writeln!(
                out,
                "{:?} {}/s x{}: acked {}/{} p99 {:.1} ms",
                result.labels.design,
                result.rate,
                result.projects,
                result.acked,
                result.sent,
                result.ingest_ms.p99
            )?;
        }
        Command::Finish(args) => {
            let result = report::finish(&args).await?;
            writeln!(out, "finished {}: lost {:?}", args.run.display(), result.lost)?;
        }
        Command::Report { dir } => {
            out.write_all(report::markdown(&report::load_runs(&dir)?).as_bytes())?;
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    let interrupt = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        else {
            interrupt.await.unwrap_or_default();
            return;
        };
        tokio::select! {
            _ = interrupt => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    interrupt.await.unwrap_or_default();
}
