//! Subcommand implementations. Each command writes its output to the writer
//! it is given, so tests can capture it.

pub(crate) mod completions;
#[cfg(feature = "test-hooks")]
pub(crate) mod dev;
pub(crate) mod migrate;
pub(crate) mod openapi;
pub(crate) mod serve;
// +scaffold:command:module

use std::future::Future;

use anyhow::Context as _;

use crate::config::Config;
use crate::paths;

/// Settings that shape how every command runs.
#[derive(Debug, Default)]
pub(crate) struct Context {
    /// The config file, read once in `main`.
    pub(crate) config: Config,
}

impl Context {
    /// The server config with `data_dir` filled in, from `flag` first, then
    /// the config file, then the XDG data directory.
    pub(crate) fn server_config(
        &self,
        data_dir: Option<&std::path::Path>,
    ) -> anyhow::Result<bizkit_core::Config> {
        let mut config = self.config.server.clone();
        let dir = match data_dir.or(config.data_dir.as_deref()) {
            Some(dir) => paths::expand_home(dir)?,
            None => paths::data_dir()?,
        };
        config.data_dir = Some(dir);
        Ok(config)
    }
}

/// Runs an async command on a fresh multi-thread runtime, so `main` stays
/// synchronous and keeps its error and exit-code handling.
pub(crate) fn block_on<T>(future: impl Future<Output = anyhow::Result<T>>) -> anyhow::Result<T> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?
        .block_on(future)
}
