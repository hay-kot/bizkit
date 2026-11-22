//! The `migrate` command.

use std::io::Write;
use std::path::PathBuf;

use super::{Context, block_on};
use crate::modules::modules;

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Where the database lives [default: from the config file, then the XDG data dir]
    #[arg(long, env = "BIZKIT_DATA_DIR")]
    data_dir: Option<PathBuf>,
}

/// Runs core and module migrations, then exits.
pub(crate) fn run(args: &Args, ctx: &Context, out: &mut impl Write) -> anyhow::Result<()> {
    block_on(migrate(args, ctx, out))
}

async fn migrate(args: &Args, ctx: &Context, out: &mut impl Write) -> anyhow::Result<()> {
    let config = ctx.server_config(args.data_dir.as_deref())?;
    let db = bizkit_core::open_db(&config, &modules()).await?;
    db.close().await;
    let dir = config.data_dir.unwrap_or_default();
    writeln!(out, "migrated {}", dir.join("bizkit.db").display())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrates_a_fresh_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let args = Args { data_dir: Some(dir.path().to_path_buf()) };
        let mut out = Vec::new();
        migrate(&args, &Context::default(), &mut out).await.unwrap();
        assert!(dir.path().join("bizkit.db").exists(), "the database file exists");
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.starts_with("migrated ") && text.trim_end().ends_with("bizkit.db"),
            "output: {text}"
        );
    }
}
