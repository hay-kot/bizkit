//! XDG directories for the application. These follow XDG on every platform,
//! including macOS, so `~/.config/bizkit` works everywhere.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use etcetera::BaseStrategy as _;
use etcetera::base_strategy::Xdg;

const APP_NAME: &str = "bizkit";

fn strategy() -> anyhow::Result<Xdg> {
    Xdg::new().context("finding the home directory")
}

/// `$XDG_DATA_HOME/<app>`, or `~/.local/share/<app>`.
pub(crate) fn data_dir() -> anyhow::Result<PathBuf> {
    Ok(strategy()?.data_dir().join(APP_NAME))
}

/// Expands a leading `~/` to the home directory.
pub(crate) fn expand_home(path: &Path) -> anyhow::Result<PathBuf> {
    match path.strip_prefix("~") {
        Ok(rest) => Ok(strategy()?.home_dir().join(rest)),
        Err(_) => Ok(path.to_path_buf()),
    }
}

/// `$XDG_CONFIG_HOME/<app>`, or `~/.config/<app>`.
pub(crate) fn config_dir() -> anyhow::Result<PathBuf> {
    Ok(strategy()?.config_dir().join(APP_NAME))
}
