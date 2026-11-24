//! The server side: an analytics beacon endpoint over one pipeline design,
//! plus the dashboard, app and rollup probes the load generator times.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use bizkit_analytics::api::Day;
use bizkit_analytics::{beacon, enrich, ua};
use bizkit_core::ids::ProjectId;
use bizkit_core::time::UnixMillis;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use crate::pipeline::{self, Design, Pipeline, PipelineStats, Rejected, Tuning};
use crate::store::{Layout, Row, Store, Sync};

/// `serve` flags.
#[derive(Debug, Clone, clap::Args)]
pub struct ServeArgs {
    /// Where the SQLite files go.
    #[arg(long, default_value = "/data")]
    pub data: PathBuf,
    /// The pipeline design.
    #[arg(long, value_enum)]
    pub design: Design,
    /// One file per project, or one shared with the app.
    #[arg(long, value_enum, default_value = "per-project")]
    pub layout: Layout,
    /// The `synchronous` pragma.
    #[arg(long, value_enum, default_value = "normal")]
    pub sync: Sync,
    /// Buffered: milliseconds between flushes.
    #[arg(long, default_value_t = 250)]
    pub flush_ms: u64,
    /// The most rows per transaction.
    #[arg(long, default_value_t = 1000)]
    pub flush_max: usize,
    /// Queue capacity per writer.
    #[arg(long, default_value_t = 10_000)]
    pub queue: usize,
    /// The listen address.
    #[arg(long, default_value = "0.0.0.0:8600")]
    pub listen: SocketAddr,
}

/// What `GET /stats` returns.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ServerStats {
    /// The pipeline's counters.
    pub pipeline: PipelineStats,
    /// Database bytes over every file.
    pub db_bytes: u64,
    /// WAL bytes over every file.
    pub wal_bytes: u64,
}

/// A probe's answer: how long the server spent on it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Timed {
    /// Milliseconds.
    pub ms: f64,
}

#[derive(Debug, Clone)]
struct AppState {
    store: Arc<Store>,
    pipeline: Arc<dyn Pipeline>,
    salt: [u8; 32],
    projects: Arc<Mutex<HashMap<u32, ProjectId>>>,
}

/// A running server.
#[derive(Debug)]
pub struct Server {
    /// The bound address.
    pub addr: SocketAddr,
    store: Arc<Store>,
    pipeline: Arc<dyn Pipeline>,
    task: tokio::task::JoinHandle<()>,
    stop: tokio::sync::oneshot::Sender<()>,
}

impl Server {
    /// Stops accepting requests, writes what the pipeline holds and closes
    /// the files: the clean shutdown a SIGKILL skips.
    pub async fn shutdown(self) {
        self.stop.send(()).unwrap_or_default();
        self.task.await.unwrap_or_default();
        self.pipeline.shutdown().await;
        self.store.close().await;
    }
}

/// Opens the store, builds the pipeline and starts serving.
///
/// # Errors
///
/// When the store can't open or the address can't be bound.
pub async fn start(args: &ServeArgs) -> anyhow::Result<Server> {
    let store = Arc::new(Store::open(&args.data, args.layout, args.sync).await?);
    let tuning = Tuning {
        flush: Duration::from_millis(args.flush_ms),
        flush_max: args.flush_max,
        queue: args.queue,
    };
    let pipeline = pipeline::build(args.design, Arc::clone(&store), tuning);
    let state = AppState {
        store: Arc::clone(&store),
        pipeline: Arc::clone(&pipeline),
        salt: [7; 32],
        projects: Arc::default(),
    };
    let router = Router::new()
        .route("/healthz", get(|| async { StatusCode::NO_CONTENT }))
        .route("/e/{project}", post(collect))
        .route("/q/{project}", get(dashboard))
        .route("/app", post(app))
        .route("/rollup/{project}", post(rollup))
        .route("/stats", get(stats))
        .with_state(state);
    let listener =
        TcpListener::bind(args.listen).await.with_context(|| format!("binding {}", args.listen))?;
    let addr = listener.local_addr()?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let serve = axum::serve(listener, router).with_graceful_shutdown(async {
            stopped.await.unwrap_or_default();
        });
        serve.await.unwrap_or_default();
    });
    Ok(Server { addr, store, pipeline, task, stop })
}

fn now() -> UnixMillis {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    UnixMillis(i64::try_from(millis).unwrap_or(i64::MAX))
}

fn today() -> String {
    Day::of(now()).to_string()
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

async fn collect(
    State(state): State<AppState>,
    Path(project): Path<u32>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Ok(row) = enrich_row(&state, project, &headers, &body) else {
        return StatusCode::BAD_REQUEST;
    };
    match state.pipeline.submit(row).await {
        Ok(()) => StatusCode::ACCEPTED,
        Err(Rejected::Full) => StatusCode::SERVICE_UNAVAILABLE,
        Err(Rejected::Failed) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// The per-beacon work production does before the write: parse, bot check,
/// UA, attribution and the visitor hash.
fn enrich_row(
    state: &AppState,
    project: u32,
    headers: &HeaderMap,
    body: &[u8],
) -> anyhow::Result<Row> {
    let beacon = beacon::parse(body).map_err(|reason| anyhow::anyhow!("{reason:?}"))?;
    let agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
    anyhow::ensure!(!ua::is_bot(agent), "bot");
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<IpAddr>().ok());
    let project_id =
        *state.projects.lock().unwrap_or_else(PoisonError::into_inner).entry(project).or_default();
    let at = now();
    let day = Day::of(at).to_string();
    let source = enrich::source(&beacon.url, beacon.referrer.as_ref());
    let parsed = ua::parse(agent, beacon.width);
    let visitor = enrich::visitor(&state.salt, project_id, enrich::Who::Web { ip, ua: agent });
    Ok(Row {
        project,
        ts: at.0,
        person: format!("v:{day}:{visitor}"),
        day,
        name: beacon.name.as_str().to_owned(),
        path: Some(enrich::path(&beacon.url)),
        host: beacon.url.host_str().map(str::to_ascii_lowercase),
        attribution: [
            source.referrer_host,
            source.utm_source,
            source.utm_medium,
            source.utm_campaign,
            None,
        ],
        agent: [parsed.device.to_owned(), parsed.browser.to_owned(), parsed.os.to_owned()],
        visitor,
        props: beacon.props.map(|props| serde_json::Value::Object(props).to_string()),
    })
}

async fn dashboard(
    State(state): State<AppState>,
    Path(project): Path<u32>,
) -> Result<Json<Timed>, StatusCode> {
    let started = Instant::now();
    state
        .store
        .dashboard(project, &today())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(Timed { ms: elapsed_ms(started) }))
}

async fn app(State(state): State<AppState>) -> StatusCode {
    match state.store.sign_in(now().0).await {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn rollup(
    State(state): State<AppState>,
    Path(project): Path<u32>,
) -> Result<Json<Timed>, StatusCode> {
    let started = Instant::now();
    state.store.rollup(project, &today()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(Timed { ms: elapsed_ms(started) }))
}

async fn stats(State(state): State<AppState>) -> Json<ServerStats> {
    let (db_bytes, wal_bytes) = state.store.sizes().await;
    Json(ServerStats { pipeline: state.pipeline.stats(), db_bytes, wal_bytes })
}
