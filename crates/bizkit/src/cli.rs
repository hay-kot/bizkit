//! Command-line interface definition.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use tracing_subscriber::filter::LevelFilter;

use crate::{build_info, commands};

#[derive(Debug, Parser)]
#[command(name = "bizkit", version = build_info::VERSION, about, long_about = None)]
pub(crate) struct Cli {
    #[command(flatten)]
    pub(crate) global: GlobalArgs,

    #[command(subcommand)]
    pub(crate) command: Command,
}

/// Flags shared by every command.
#[derive(Debug, Args)]
pub(crate) struct GlobalArgs {
    /// Log level
    #[arg(long, env = "LOG_LEVEL", value_enum, global = true)]
    pub(crate) log_level: Option<LogLevel>,

    /// Log every SQL statement at debug. Without it, SQL logs only slow
    /// statements and errors, as warnings
    #[arg(long, env = "LOG_SQL", global = true)]
    pub(crate) log_sql: bool,

    /// Disable colored output
    #[arg(long, env = "NO_COLOR", global = true)]
    pub(crate) no_color: bool,

    /// Path to the config file [default: config directory]
    #[arg(long, env = "CONFIG_FILE", global = true)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Serve the API, MCP endpoint and web app
    Serve(Box<commands::serve::Args>),
    /// Run database migrations, then exit
    Migrate(commands::migrate::Args),
    /// Print the OpenAPI document as JSON
    Openapi(commands::openapi::Args),
    /// Development and test helpers
    #[cfg(feature = "test-hooks")]
    Dev(commands::dev::Args),
    // +scaffold:command:variant
    /// Print a shell completion script
    Completions(commands::completions::Args),
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => Self::TRACE,
            LogLevel::Debug => Self::DEBUG,
            LogLevel::Info => Self::INFO,
            LogLevel::Warn => Self::WARN,
            LogLevel::Error => Self::ERROR,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory as _;

    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
