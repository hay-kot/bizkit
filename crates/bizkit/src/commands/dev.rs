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
    /// Serve the demo product site, tracked with the demo workspace's
    /// website snippet, until interrupted
    #[cfg(feature = "analytics")]
    DemoSite {
        /// The address to listen on
        #[arg(long)]
        listen: std::net::SocketAddr,
        /// Where browsers reach bizkit's public routes, such as the dev UI
        #[arg(long)]
        snippet_base: Url,
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
            let data_dir = config.data_dir.as_deref().context("data_dir is not set")?;
            bizkit_core::dev::demo::seed(&db, &modules, SystemClock.now(), data_dir).await?;
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
        #[cfg(feature = "analytics")]
        DevCommand::DemoSite { listen, snippet_base } => {
            let key = bizkit_analytics::demo_site_key(&db).await?.context(
                "the demo workspace has no website site; run `bizkit dev seed-demo` first",
            )?;
            db.close().await;
            let tag = bizkit_analytics::demo_site::snippet_tag(snippet_base, &key);
            let listener = tokio::net::TcpListener::bind(listen)
                .await
                .with_context(|| format!("binding {listen}"))?;
            let bound = listener.local_addr().context("reading the bound address")?;
            writeln!(out, "serving the demo site on http://{bound}")?;
            out.flush()?;
            bizkit_analytics::demo_site::serve(listener, &tag, super::serve::wait_for_signal())
                .await
                .context("serving the demo site")?;
            return Ok(());
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

    #[cfg(feature = "analytics")]
    #[tokio::test]
    async fn demo_site_needs_the_demo_seed() {
        let dir = tempfile::tempdir().unwrap();
        let args = Args {
            command: DevCommand::DemoSite {
                listen: "127.0.0.1:0".parse().unwrap(),
                snippet_base: Url::parse("http://localhost:5173").unwrap(),
            },
            data_dir: Some(dir.path().to_path_buf()),
        };
        let mut out = Vec::new();
        let err = dev(&args, &Context::default(), &mut out).await.unwrap_err();
        assert!(format!("{err:#}").contains("run `bizkit dev seed-demo` first"), "{err:#}");
        assert_eq!(out, b"", "nothing is printed");
    }

    /// Hands each write to the test while `dev` keeps running.
    #[cfg(feature = "analytics")]
    struct Sender(tokio::sync::mpsc::UnboundedSender<Vec<u8>>);

    #[cfg(feature = "analytics")]
    impl Write for Sender {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.send(buf.to_vec()).map_err(std::io::Error::other)?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[cfg(feature = "analytics")]
    #[tokio::test]
    async fn demo_site_serves_pages_with_the_snippet() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let seed = Args { command: DevCommand::SeedDemo, data_dir: Some(data_dir.clone()) };
        dev(&seed, &Context::default(), &mut Vec::new()).await.unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let args = Args {
            command: DevCommand::DemoSite {
                listen: "127.0.0.1:0".parse().unwrap(),
                snippet_base: Url::parse("http://localhost:5173").unwrap(),
            },
            data_dir: Some(data_dir),
        };
        let server = tokio::spawn(async move {
            dev(&args, &Context::default(), &mut Sender(tx)).await.unwrap();
        });
        let mut line = Vec::new();
        while !line.ends_with(b"\n") {
            line.extend(rx.recv().await.unwrap());
        }
        let line = String::from_utf8(line).unwrap();
        let addr = line
            .strip_prefix("serving the demo site on http://")
            .and_then(|rest| rest.strip_suffix('\n'))
            .unwrap_or_else(|| panic!("output: {line:?}"))
            .to_owned();
        let page = tokio::task::spawn_blocking(move || {
            use std::io::Read as _;
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            stream
                .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .unwrap();
            let mut page = String::new();
            stream.read_to_string(&mut page).unwrap();
            page
        })
        .await
        .unwrap();
        server.abort();
        assert!(page.starts_with("HTTP/1.1 200"), "{page}");
        assert!(
            page.contains(r#"<script defer src="http://localhost:5173/m/analytics/public/s/"#),
            "{page}"
        );
    }

    /// The website site takes the demo site's host and key, and its file
    /// has onboarding events, none in the future.
    #[cfg(feature = "analytics")]
    async fn demo_site_is_ready(data_dir: &std::path::Path) {
        let db = bizkit_core::db::Db::open(&data_dir.join("bizkit.db")).await.unwrap();
        let (project, hostnames, rolled_through): (
            bizkit_core::ids::ProjectId,
            String,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT s.project_id, s.hostnames, s.rolled_through FROM analytics_sites s
             JOIN projects p ON p.id = s.project_id WHERE p.slug = 'website'",
        )
        .fetch_one(db.read())
        .await
        .unwrap();
        assert!(hostnames.contains("\"localhost\""), "{hostnames}");
        assert!(bizkit_analytics::demo_site_key(&db).await.unwrap().is_some());
        db.close().await;
        let file = bizkit_analytics::testing::project_pool(data_dir, project).await.unwrap();
        let (onboarded, latest): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*) FILTER (WHERE name = 'report_viewed'), MAX(ts) FROM analytics_events",
        )
        .fetch_one(&file)
        .await
        .unwrap();
        let last_rolled: Option<String> =
            sqlx::query_scalar("SELECT MAX(day) FROM analytics_daily")
                .fetch_one(&file)
                .await
                .unwrap();
        file.close().await;
        assert!(onboarded > 0, "some people finish onboarding");
        assert!(rolled_through.is_some(), "the seeded days before today are rolled up");
        assert_eq!(last_rolled, rolled_through, "daily rows up to rolled_through");
        let now = SystemClock.now().0;
        assert!(latest <= now, "no event after the seed ran: {latest} > {now}");
    }

    #[cfg(all(feature = "mailing", feature = "forms"))]
    #[tokio::test]
    async fn seed_demo_fills_mailing_once() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let args = Args { command: DevCommand::SeedDemo, data_dir: Some(data_dir.clone()) };
        let counts = || async {
            let db = bizkit_core::db::Db::open(&data_dir.join("bizkit.db")).await.unwrap();
            let mut counts = Vec::new();
            for sql in [
                "SELECT COUNT(*) FROM mailing_lists",
                "SELECT COUNT(*) FROM mailing_list_forms",
                "SELECT COUNT(*) FROM mailing_contacts WHERE status = 'subscribed'",
                "SELECT COUNT(*) FROM mailing_contacts",
                "SELECT COUNT(*) FROM mailing_broadcasts",
                "SELECT COUNT(*) FROM mailing_deliveries",
                "SELECT COUNT(*) FROM mailing_sequences",
                "SELECT COUNT(*) FROM mailing_steps",
                "SELECT COUNT(*) FROM mailing_enrollments WHERE status = 'active'",
                "SELECT COUNT(*) FROM mailing_enrollments",
                "SELECT COUNT(*) FROM mailing_suppressions",
                "SELECT COUNT(*) FROM mailing_deliveries WHERE event = 'bounced'",
                "SELECT COUNT(*) FROM jobs",
            ] {
                counts.push(sqlx::query_scalar::<_, i64>(sql).fetch_one(db.read()).await.unwrap());
            }
            db.close().await;
            counts
        };
        dev(&args, &Context::default(), &mut Vec::new()).await.unwrap();
        let first = counts().await;
        assert_eq!(
            first[..12],
            [1, 1, 3, 5, 2, 8, 1, 3, 0, 2, 2, 1],
            "lists, form links, subscribed, contacts, broadcasts, deliveries (3 broadcast, 5 step), \
             sequences, steps, active and all enrollments, suppressions (one manual, one bounce), \
             bounced deliveries"
        );
        dev(&args, &Context::default(), &mut Vec::new()).await.unwrap();
        assert_eq!(counts().await, first, "a second run changes nothing and queues no mail");
    }

    #[cfg(feature = "analytics")]
    #[tokio::test]
    async fn seed_demo_fills_analytics_once() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let args = Args { command: DevCommand::SeedDemo, data_dir: Some(data_dir.clone()) };
        let counts = || async {
            let db = bizkit_core::db::Db::open(&data_dir.join("bizkit.db")).await.unwrap();
            let mut counts = Vec::new();
            for sql in [
                "SELECT COUNT(*) FROM analytics_sites",
                "SELECT COUNT(*) FROM analytics_goals",
                "SELECT COUNT(*) FROM analytics_funnels",
            ] {
                counts.push(sqlx::query_scalar::<_, i64>(sql).fetch_one(db.read()).await.unwrap());
            }
            let projects: Vec<bizkit_core::ids::ProjectId> =
                sqlx::query_scalar("SELECT project_id FROM analytics_sites ORDER BY project_id")
                    .fetch_all(db.read())
                    .await
                    .unwrap();
            db.close().await;
            // Events, people and conversions live in each project's file.
            let mut file_counts = [0_i64; 4];
            for project in projects {
                let file =
                    bizkit_analytics::testing::project_pool(&data_dir, project).await.unwrap();
                for (at, sql) in [
                    "SELECT COUNT(DISTINCT day) FROM analytics_events",
                    "SELECT COUNT(*) FROM analytics_people",
                    "SELECT COUNT(*) FROM analytics_conversions",
                    "SELECT COUNT(*) FROM analytics_events",
                ]
                .into_iter()
                .enumerate()
                {
                    file_counts[at] +=
                        sqlx::query_scalar::<_, i64>(sql).fetch_one(&file).await.unwrap();
                }
                file.close().await;
            }
            counts.extend(file_counts);
            counts
        };
        dev(&args, &Context::default(), &mut Vec::new()).await.unwrap();
        let first = counts().await;
        assert_eq!(
            first[..4],
            [2, 2, 2, 28],
            "sites, goals, funnels, days summed over two sites: {first:?}"
        );
        assert!(
            first[4..].iter().all(|count| *count > 0),
            "people, conversions, events: {first:?}"
        );
        dev(&args, &Context::default(), &mut Vec::new()).await.unwrap();
        assert_eq!(counts().await, first, "a second run changes nothing");
        demo_site_is_ready(&data_dir).await;

        // The move to project files dropped the events and reset the day
        // markers but kept the sites; a run after it refills the files.
        std::fs::remove_dir_all(data_dir.join("analytics")).unwrap();
        let db = bizkit_core::db::Db::open(&data_dir.join("bizkit.db")).await.unwrap();
        let mut tx = db.write().await.unwrap();
        sqlx::query("UPDATE analytics_sites SET last_event_day = NULL")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        db.close().await;
        dev(&args, &Context::default(), &mut Vec::new()).await.unwrap();
        assert_eq!(counts().await, first, "a run after the upgrade refills the files");
        let reopened = bizkit_core::db::Db::open(&data_dir.join("bizkit.db")).await.unwrap();
        let unmarked: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM analytics_sites WHERE last_event_day IS NULL")
                .fetch_one(reopened.read())
                .await
                .unwrap();
        reopened.close().await;
        assert_eq!(unmarked, 0, "refilled sites report events");
    }
}
