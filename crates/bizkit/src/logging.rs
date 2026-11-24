//! Tracing setup. Logs go to stderr so stdout stays free for command output.

use anyhow::Context as _;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

use crate::cli::LogLevel;

/// Installs the global subscriber, logging to stderr.
pub(crate) fn init(level: LogLevel, sql: bool, color: bool) -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(filter(level, sql))
        .with(fmt::layer().with_writer(std::io::stderr).with_ansi(color))
        .try_init()
        .context("installing the tracing subscriber")
}

/// `level` for everything, except that sqlx logs each statement at debug: at
/// debug that is several lines a second from the idle job worker alone. sqlx
/// stays at warn, which keeps its slow-statement warnings, unless `sql`.
fn filter(level: LogLevel, sql: bool) -> Targets {
    let level = LevelFilter::from(level);
    let sqlx = if sql { level } else { level.min(LevelFilter::WARN) };
    Targets::new().with_default(level).with_target("sqlx", sqlx)
}

#[cfg(test)]
mod tests {
    use tracing::Level;

    use super::*;

    #[test]
    fn sql_statements_need_log_sql() {
        let quiet = filter(LogLevel::Debug, false);
        assert!(quiet.would_enable("bizkit_core::jobs", &Level::DEBUG), "app debug logs stay");
        assert!(!quiet.would_enable("sqlx::query", &Level::DEBUG), "statements are off");
        assert!(quiet.would_enable("sqlx::query", &Level::WARN), "slow statements stay");

        let loud = filter(LogLevel::Debug, true);
        assert!(loud.would_enable("sqlx::query", &Level::DEBUG), "--log-sql turns them on");

        let info = filter(LogLevel::Info, true);
        assert!(!info.would_enable("sqlx::query", &Level::DEBUG), "never above the level");
    }
}
