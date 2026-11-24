//! The `serve` command.

use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context as _;
use bizkit_core::{App, Deps};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::{Context, block_on};
use crate::modules::modules;

const DEFAULT_LISTEN: &str = "127.0.0.1:8484";

#[derive(Debug, Default, clap::Args)]
pub(crate) struct Args {
    /// Address to listen on [default: from the config file, then 127.0.0.1:8484]
    #[arg(long, env = "BIZKIT_LISTEN")]
    listen: Option<SocketAddr>,

    /// Public URL of the server, for links and the CSRF check [default: http:// and the bound address]
    #[arg(long, env = "BIZKIT_BASE_URL")]
    base_url: Option<Url>,

    /// Where the database and blobs live [default: from the config file, then the XDG data dir]
    #[arg(long, env = "BIZKIT_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// SMTP URL for outgoing mail, overriding the config file
    #[arg(long, env = "BIZKIT_SMTP_URL")]
    smtp_url: Option<Url>,
}

/// Serves until ctrl-c or SIGTERM.
pub(crate) fn run(args: &Args, ctx: &Context, out: &mut impl Write) -> anyhow::Result<()> {
    block_on(async {
        let shutdown = CancellationToken::new();
        let signal = shutdown.clone();
        tokio::spawn(async move {
            wait_for_signal().await;
            tracing::info!("shutting down");
            signal.cancel();
        });
        serve(args, ctx, out, shutdown).await
    })
}

pub(crate) async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let Ok(mut term) = signal(SignalKind::terminate()) else {
            // Without a SIGTERM handler, ctrl-c still works.
            let _ignored = tokio::signal::ctrl_c().await;
            return;
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ignored = tokio::signal::ctrl_c().await;
    }
}

/// Builds the app, binds, prints the bound address and serves until
/// `shutdown` fires.
pub(crate) async fn serve(
    args: &Args,
    ctx: &Context,
    out: &mut impl Write,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let mut config = ctx.server_config(args.data_dir.as_deref())?;
    if let Some(listen) = args.listen {
        config.listen = Some(listen);
    }
    if let Some(base_url) = &args.base_url {
        config.base_url = Some(base_url.clone());
    }
    if let Some(smtp_url) = &args.smtp_url {
        config.mail.smtp_url = Some(smtp_url.clone());
    }
    if let Some(var) = &config.mail.webhook_signing_key_env {
        let key = std::env::var(var)
            .with_context(|| format!("reading the mail webhook signing key from ${var}"))?;
        config.mail.webhook_signing_key = Some(key);
    }
    let listen = match config.listen {
        Some(listen) => listen,
        None => DEFAULT_LISTEN.parse().context("parsing the default listen address")?,
    };
    let listener =
        tokio::net::TcpListener::bind(listen).await.with_context(|| format!("binding {listen}"))?;
    let addr = listener.local_addr().context("reading the bound address")?;
    if config.base_url.is_none() {
        config.base_url = Some(format!("http://{addr}").parse().context("building the base URL")?);
    }

    let deps = Deps::from_config(&config)?;
    // One clock serves both the app and `/__test/clock`, so e2e tests can move
    // the server's time.
    #[cfg(feature = "test-hooks")]
    let clock = std::sync::Arc::new(bizkit_core::test_hooks::OffsetClock::default());
    #[cfg(feature = "test-hooks")]
    let deps = Deps {
        clock: std::sync::Arc::<bizkit_core::test_hooks::OffsetClock>::clone(&clock),
        ..deps
    };

    let app = App::build(config, deps, modules()).await?;
    let router = app.router();
    #[cfg(feature = "test-hooks")]
    let router = router.merge(bizkit_core::test_hooks::router(clock, app.worker().clone()));
    let router = router.fallback_service(bizkit_web::router());

    writeln!(out, "listening on http://{addr}")?;
    out.flush()?;
    app.serve(router, listener, shutdown).await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn serves_until_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let args = Args {
            listen: Some("127.0.0.1:0".parse().unwrap()),
            data_dir: Some(dir.path().to_path_buf()),
            ..Args::default()
        };
        let shutdown = CancellationToken::new();
        let stop = shutdown.clone();
        let mut out = Vec::new();
        let ctx = Context::default();
        let (served, ()) = tokio::join!(serve(&args, &ctx, &mut out, shutdown), async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            stop.cancel();
        });
        served.unwrap();
        let text = String::from_utf8(out).unwrap();
        insta::with_settings!({ filters => vec![(r"127\.0\.0\.1:\d+", "127.0.0.1:[port]")] }, {
            insta::assert_snapshot!(text, @"listening on http://127.0.0.1:[port]");
        });
    }
}
