//! XDG directories for the application. These follow XDG on every platform,
//! including macOS, so `~/.config/bizkit` works everywhere.

use std::path::PathBuf;

use anyhow::Context as _;
use etcetera::BaseStrategy as _;
use etcetera::base_strategy::Xdg;

const APP_NAME: &str = "bizkit";

fn strategy() -> anyhow::Result<Xdg> {
    Xdg::new().context("finding the home directory")
}

/// `$XDG_CONFIG_HOME/<app>`, or `~/.config/<app>`.
pub(crate) fn config_dir() -> anyhow::Result<PathBuf> {
    Ok(strategy()?.config_dir().join(APP_NAME))
}
