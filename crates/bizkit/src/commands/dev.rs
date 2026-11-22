//! The `dev` command: seeding and sign-in shortcuts for development and
//! end-to-end tests. Built only with the `test-hooks` feature.

use std::io::Write;
use std::path::PathBuf;

use anyhow::Context as _;
use bizkit_core::deps::{Clock as _, SystemClock};
use bizkit_core::dev::personas::{self, Persona, SeedOptions};
use url::Url;

use super::{Context, block_on};
use crate::modules::modules;

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(subcommand)]
    command: DevCommand,

    /// Where the database lives [default: from the config file, then the XDG data dir]
    #[arg(long, env = "BIZKIT_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,
}

#[derive(Debug, clap::Subcommand)]
enum DevCommand {
    /// Seed a persona's rows straight into the database
    SeedPersona {
        /// basic or team
        #[arg(long)]
        persona: Persona,
        /// Goes into every email and slug so parallel tests never collide
        #[arg(long)]
        namespace: String,
        /// Write the manifest here instead of to stdout
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Seed the demo workspace and each module's sample data. Safe to run again
    SeedDemo,
    /// Print a sign-in link for an email without sending mail
    Login {
        /// The email to sign in as
        email: String,
    },
}

pub(crate) fn run(args: &Args, ctx: &Context, out: &mut impl Write) -> anyhow::Result<()> {
    block_on(dev(args, ctx, out))
}

async fn dev(args: &Args, ctx: &Context, out: &mut impl Write) -> anyhow::Result<()> {
    let config = ctx.server_config(args.data_dir.as_deref())?;
    let modules = modules();
    let db = bizkit_core::open_db(&config, &modules).await?;
    match &args.command {
        DevCommand::SeedPersona { persona, namespace, out: path } => {
            let areas = bizkit_core::scope_areas(&modules)?;
            let session_ttl =
                std::time::Duration::from_hours(u64::from(config.auth.session_ttl_days) * 24);
            let options = SeedOptions { areas: &areas, now: SystemClock.now(), session_ttl };
            let manifest = personas::seed(&db, &options, *persona, namespace).await?;
            let json =
                serde_json::to_string_pretty(&manifest).context("serializing the manifest")?;
            match path {
                Some(path) => std::fs::write(path, json)
                    .with_context(|| format!("writing {}", path.display()))?,
                None => writeln!(out, "{json}")?,
            }
        }
        DevCommand::SeedDemo => {
            bizkit_core::dev::demo::seed(&db, &modules, SystemClock.now()).await?;
            writeln!(
                out,
                "seeded the demo workspace; sign in as {}",
                bizkit_core::dev::demo::DEMO_EMAIL
            )?;
        }
        DevCommand::Login { email } => {
            let base_url = match &config.base_url {
                Some(url) => url.clone(),
                None => {
                    Url::parse("http://127.0.0.1:8484").context("parsing the default base URL")?
                }
            };
            let link =
                bizkit_core::dev::login_link(&db, &base_url, email, SystemClock.now()).await?;
            writeln!(out, "{link}")?;
        }
    }
    db.close().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn seed_persona_writes_every_key() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("manifest.json");
        let args = Args {
            command: DevCommand::SeedPersona {
                persona: Persona::Team,
                namespace: "ns1".to_owned(),
                out: Some(manifest_path.clone()),
            },
            data_dir: Some(dir.path().join("data")),
        };
        dev(&args, &Context::default(), &mut Vec::new()).await.unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
        insta::assert_json_snapshot!(manifest, {
            ".accounts.*.id" => "[id]",
            ".accounts.*.session_token" => "[token]",
            ".workspaces.*.id" => "[id]",
            ".projects.*.id" => "[id]",
            ".api_keys.*.id" => "[id]",
            ".api_keys.*.token" => "[token]",
            ".invitations.*.id" => "[id]",
        });
    }

    #[cfg(feature = "forms")]
    async fn counts(data_dir: &std::path::Path) -> Vec<i64> {
        let db = bizkit_core::db::Db::open(&data_dir.join("bizkit.db")).await.unwrap();
        let mut counts = Vec::new();
        for sql in [
            "SELECT COUNT(*) FROM users",
            "SELECT COUNT(*) FROM workspaces",
            "SELECT COUNT(*) FROM memberships",
            "SELECT COUNT(*) FROM projects",
            "SELECT COUNT(*) FROM forms",
            "SELECT COUNT(*) FROM form_submissions",
        ] {
            counts.push(sqlx::query_scalar::<_, i64>(sql).fetch_one(db.read()).await.unwrap());
        }
        db.close().await;
        counts
    }

    #[cfg(feature = "forms")]
    #[tokio::test]
    async fn seed_demo_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let args = Args { command: DevCommand::SeedDemo, data_dir: Some(data_dir.clone()) };
        let mut out = Vec::new();
        dev(&args, &Context::default(), &mut out).await.unwrap();
        let first = counts(&data_dir).await;
        assert_eq!(
            first,
            [1, 1, 1, 2, 2, 5],
            "users, workspaces, memberships, projects, forms, submissions"
        );
        dev(&args, &Context::default(), &mut Vec::new()).await.unwrap();
        assert_eq!(counts(&data_dir).await, first, "a second run changes nothing");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "seeded the demo workspace; sign in as demo@bizkit.test\n",
            "output"
        );
    }
}
