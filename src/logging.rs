//! Tracing setup. Logs go to stderr so stdout stays free for command output.

use anyhow::Context as _;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

use crate::cli::LogLevel;

/// Installs the global subscriber, logging to stderr.
pub(crate) fn init(level: LogLevel, color: bool) -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(LevelFilter::from(level))
        .with(fmt::layer().with_writer(std::io::stderr).with_ansi(color))
        .try_init()
        .context("installing the tracing subscriber")
}
