//! Self-hosted business hub for small software companies: analytics, forms, webhooks and document signing

mod build_info;
mod cli;
mod commands;
mod config;
mod logging;
mod modules;
mod paths;

use std::io::{self, IsTerminal as _};
use std::process::ExitCode;

use clap::{CommandFactory as _, FromArgMatches as _};

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let matches = Cli::command().long_version(build_info::long_version()).get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(err) => err.exit(),
    };

    let color = !cli.global.no_color && io::stderr().is_terminal();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            report(&err, color);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let config = match &cli.global.config {
        Some(path) => config::Config::read_from(path)?,
        None => config::Config::read()?,
    };
    let log_level = cli.global.log_level.or(config.log_level).unwrap_or_default();
    logging::init(log_level, cli.global.log_sql, !cli.global.no_color)?;
    let ctx = commands::Context { config };
    let mut out = io::stdout().lock();

    match cli.command {
        Command::Serve(args) => commands::serve::run(&args, &ctx, &mut out),
        Command::Migrate(args) => commands::migrate::run(&args, &ctx, &mut out),
        Command::Openapi(args) => commands::openapi::run(&args, &mut out),
        #[cfg(feature = "test-hooks")]
        Command::Dev(args) => commands::dev::run(&args, &ctx, &mut out),
        // +scaffold:command:dispatch
        Command::Completions(args) => {
            commands::completions::run(&args, &mut out);
            Ok(())
        }
    }
}

/// Writes `err` and its causes to stderr as `error: <message>: <cause>`.
fn report(err: &anyhow::Error, color: bool) {
    // Nothing useful remains to do if stderr itself is gone.
    let _ignored = write_report(&mut io::stderr().lock(), err, color);
}

fn write_report(out: &mut impl io::Write, err: &anyhow::Error, color: bool) -> io::Result<()> {
    let (red, reset) = if color { ("\x1b[1;31m", "\x1b[0m") } else { ("", "") };
    writeln!(out, "{red}error:{reset} {err:#}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_prints_the_error_and_its_causes() {
        let err = anyhow::anyhow!("file not found").context("loading config");
        let mut out = Vec::new();
        write_report(&mut out, &err, false).unwrap();
        insta::assert_snapshot!(
            String::from_utf8(out).unwrap(),
            @"error: loading config: file not found"
        );
    }
}
