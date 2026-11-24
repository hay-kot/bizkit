//! The application: platform state, router and background worker.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, bail};
use axum::http::StatusCode;
use axum::response::{IntoResponse as _, Response};
use axum::routing::{any, get};
use axum::{Extension, Json, Router, middleware};
use sqlx::migrate::Migrator;
use tokio_util::sync::CancellationToken;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use url::Url;
use utoipa::OpenApi as _;

use crate::auth::middleware::{authenticate, require_api_key};
use crate::auth::policies::Policies;
use crate::auth::routes::AuthDocs;
use crate::config::Config;
use crate::db::Db;
use crate::deps::Deps;
use crate::error::ApiError;
use crate::hooks::Hooks;
use crate::jobs::{Bus, Worker};
use crate::mail::MailRouter;
use crate::module::{Lifecycle, Module, Registration, register_all};
use crate::ops::{Registry, ScopeAreas};
use crate::secrets::{SecretKey, Secrets};
use crate::tenancy::workspaces;
use crate::time::UnixMillis;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared platform state. Cheap to clone; modules capture clones.
#[derive(Debug, Clone)]
pub struct Platform(Arc<PlatformInner>);

#[derive(Debug)]
struct PlatformInner {
    config: Config,
    base_url: Url,
    db: Db,
    deps: Deps,
    bus: Bus,
    policies: Policies,
    scope_areas: ScopeAreas,
    modules: Vec<&'static str>,
    hooks: Hooks,
    lifecycles: Vec<Arc<dyn Lifecycle>>,
    secrets: Option<Secrets>,
    mail: MailRouter,
}

impl Platform {
    /// The config `App::build` started with.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.0.config
    }

    /// The public URL of the server.
    #[must_use]
    pub fn base_url(&self) -> &Url {
        &self.0.base_url
    }

    /// The database.
    #[must_use]
    pub fn db(&self) -> &Db {
        &self.0.db
    }

    /// Side effects: clock, mailer, HTTP and blobs.
    #[must_use]
    pub fn deps(&self) -> &Deps {
        &self.0.deps
    }

    /// Seals and opens secrets at rest, or `None` when the instance has no
    /// secrets key.
    #[must_use]
    pub fn secrets(&self) -> Option<&Secrets> {
        self.0.secrets.as_ref()
    }

    /// Picks the mailer for each mail. Send workspace mail through this,
    /// never through `deps().mailer`.
    #[must_use]
    pub fn mail(&self) -> &MailRouter {
        &self.0.mail
    }

    /// Writes jobs and events into write transactions.
    #[must_use]
    pub fn bus(&self) -> &Bus {
        &self.0.bus
    }

    /// The current time.
    #[must_use]
    pub fn now(&self) -> UnixMillis {
        self.0.deps.clock.now()
    }

    /// The compiled email policies.
    #[must_use]
    pub fn policies(&self) -> &Policies {
        &self.0.policies
    }

    /// Every declared scope area.
    #[must_use]
    pub fn scope_areas(&self) -> &ScopeAreas {
        &self.0.scope_areas
    }

    /// The names of the compiled-in modules, core excluded.
    #[must_use]
    pub fn modules(&self) -> &[&'static str] {
        &self.0.modules
    }

    pub(crate) fn hooks(&self) -> &Hooks {
        &self.0.hooks
    }

    /// Runs every module's [`Lifecycle::flush`] in module order, so
    /// buffered work becomes visible to reads and jobs.
    ///
    /// # Errors
    ///
    /// The first module flush that fails.
    pub async fn flush_modules(&self) -> anyhow::Result<()> {
        for lifecycle in &self.0.lifecycles {
            lifecycle.flush().await?;
        }
        Ok(())
    }

    async fn stop_modules(&self) {
        for lifecycle in &self.0.lifecycles {
            lifecycle.stop().await;
        }
    }

    /// How long a session lasts without use.
    #[must_use]
    pub fn session_ttl(&self) -> Duration {
        Duration::from_hours(u64::from(self.0.config.auth.session_ttl_days) * 24)
    }
}

/// The application, ready to serve.
#[derive(Debug)]
pub struct App {
    platform: Platform,
    registry: Arc<Registry>,
    worker: Worker,
    spec: serde_json::Value,
    routes: Vec<crate::module::ModuleRoutes>,
    shutdown: CancellationToken,
}

/// Core's migrations, then each module's.
#[must_use]
pub fn migrators(modules: &[Box<dyn Module>]) -> Vec<Migrator> {
    let mut migrators = vec![sqlx::migrate!()];
    migrators.extend(modules.iter().filter_map(|module| module.migrations()));
    migrators
}

/// Opens the database in `config.data_dir` and runs every migration.
///
/// # Errors
///
/// When `data_dir` is unset or can't be created, the database can't open, or a migration fails.
pub async fn open_db(config: &Config, modules: &[Box<dyn Module>]) -> anyhow::Result<Db> {
    let data_dir = config.data_dir.as_deref().context("data_dir is not set")?;
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("creating {}", data_dir.display()))?;
    let db = Db::open(&data_dir.join("bizkit.db")).await?;
    db.migrate(migrators(modules)).await?;
    Ok(db)
}

/// Builds the spec from module registration alone: no config, no database.
///
/// # Errors
///
/// When a module fails to register.
pub fn openapi(modules: &[Box<dyn Module>]) -> anyhow::Result<utoipa::openapi::OpenApi> {
    let registration = register_all(modules, None)?;
    let spec = spec_json(&registration)?;
    serde_json::from_value(spec).context("building the OpenAPI document")
}

/// The scope areas core and `modules` declare, without building an app.
///
/// # Errors
///
/// When a module fails to register.
pub fn scope_areas(modules: &[Box<dyn Module>]) -> anyhow::Result<ScopeAreas> {
    Ok(register_all(modules, None)?.registry.areas().clone())
}

fn spec_json(registration: &Registration) -> anyhow::Result<serde_json::Value> {
    let mut extra = vec![serde_json::to_value(AuthDocs::openapi())?];
    for routes in &registration.routes {
        extra.push(nest_docs(&routes.nest(), &routes.docs));
    }
    crate::ops::openapi_document(&registration.registry, &extra)
}

/// Prefixes a module's documented paths with its nest.
fn nest_docs(nest: &str, docs: &serde_json::Value) -> serde_json::Value {
    let mut docs = docs.clone();
    if let Some(paths) = docs.get_mut("paths").and_then(serde_json::Value::as_object_mut) {
        let nested = std::mem::take(paths)
            .into_iter()
            .map(|(path, item)| {
                let path = if path == "/" { String::new() } else { path };
                (format!("{nest}{path}"), item)
            })
            .collect();
        *paths = nested;
    }
    docs
}

impl App {
    /// Opens the database, runs core then module migrations, and registers
    /// core then `modules` in order. `data_dir` and `base_url` must be set.
    ///
    /// # Errors
    ///
    /// When the config is invalid, `base_url` or `data_dir` is unset, a `modules` key names no compiled-in module, a module fails to register, or the database can't open or migrate.
    pub async fn build(
        config: Config,
        deps: Deps,
        modules: Vec<Box<dyn Module>>,
    ) -> anyhow::Result<Self> {
        let warnings = config.validate()?;
        for warning in &warnings {
            tracing::warn!("config: {warning}");
        }
        let base_url = config.base_url.clone().context("base_url is not set")?;
        let secrets = config
            .secrets_key
            .as_deref()
            .map(SecretKey::from_base64)
            .transpose()
            .context("reading the secrets key (secrets_key_env)")?
            .map(|key| Secrets::new(&key));
        let names: Vec<&'static str> = modules.iter().map(|module| module.name()).collect();
        for key in config.modules.keys() {
            if !names.contains(&key.as_str()) {
                bail!(
                    "config modules.{key} names no compiled-in module (have: {})",
                    names.join(", ")
                );
            }
        }
        let registration = register_all(&modules, Some(&config.modules))?;
        let spec = spec_json(&registration)?;
        let db = open_db(&config, &modules).await?;
        let policies = Policies::from_config(&config.auth.policies)?;
        for policy in policies.iter() {
            for join in &policy.join {
                if workspaces::find_by_slug(db.read(), &join.workspace).await?.is_none() {
                    tracing::warn!(
                        policy = %policy.name,
                        workspace = %join.workspace,
                        "policy joins a workspace that doesn't exist; sign-ups skip it until it does"
                    );
                }
            }
        }
        let Registration { registry, handlers, routes, hooks, lifecycles, .. } = registration;
        let handlers = Arc::new(handlers);
        let bus = Bus::new(Arc::clone(&handlers), Arc::clone(&deps.clock));
        let platform = Platform(Arc::new(PlatformInner {
            base_url,
            db,
            deps,
            bus,
            policies,
            scope_areas: registry.areas().clone(),
            modules: names,
            hooks: hooks.finish(),
            lifecycles,
            secrets,
            mail: MailRouter::default(),
            config,
        }));
        let worker = Worker::new(platform.clone(), handlers);
        Ok(Self {
            platform,
            registry: Arc::new(registry),
            worker,
            spec,
            routes,
            shutdown: CancellationToken::new(),
        })
    }

    /// API, MCP, auth and module routes. No SPA fallback: the bin adds
    /// `bizkit-web`.
    pub fn router(&self) -> Router {
        let platform = &self.platform;
        let spec = Arc::new(self.spec.clone());
        let mcp = crate::ops::mcp::service(&self.registry, platform, self.shutdown.child_token());
        let mut router = crate::ops::http::router(&self.registry, platform)
            .merge(crate::auth::routes::router())
            .route(
                "/api/openapi.json",
                get(move || async move { Json(Arc::clone(&spec)).into_response() }),
            )
            .route("/healthz", get(|| async { "ok" }))
            .merge(
                Router::new()
                    .route_service("/mcp", mcp)
                    .layer(middleware::from_fn(require_api_key)),
            )
            .route("/api/{*rest}", any(not_found))
            .route("/mcp/{*rest}", any(not_found));
        for routes in self.routes.iter().filter(|routes| !routes.public) {
            router = router.nest(&routes.nest(), routes.router.clone());
        }
        router = router.layer(middleware::from_fn_with_state(platform.clone(), authenticate));
        // Merged after the authenticate layer, so it never wraps them.
        for routes in self.routes.iter().filter(|routes| routes.public) {
            router = router.nest(&routes.nest(), routes.router.clone());
        }
        router
            .layer(Extension(platform.clone()))
            .layer(RequestBodyLimitLayer::new(crate::ops::http::MAX_BODY_BYTES))
            .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, REQUEST_TIMEOUT))
            .layer(TraceLayer::new_for_http())
            .layer(PropagateRequestIdLayer::x_request_id())
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
    }

    /// The operation registry.
    #[must_use]
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// The platform state.
    #[must_use]
    pub const fn platform(&self) -> &Platform {
        &self.platform
    }

    /// The background worker.
    #[must_use]
    pub const fn worker(&self) -> &Worker {
        &self.worker
    }

    /// The OpenAPI document, the same one [`openapi`] builds.
    ///
    /// # Errors
    ///
    /// Never in practice: the document was built when the app was.
    pub fn openapi(&self) -> anyhow::Result<utoipa::openapi::OpenApi> {
        serde_json::from_value(self.spec.clone()).context("building the OpenAPI document")
    }

    /// The OpenAPI document as JSON.
    #[must_use]
    pub const fn openapi_json(&self) -> &serde_json::Value {
        &self.spec
    }

    /// Serves `router` (the bin's, with the fallback attached) and runs the
    /// worker until `shutdown` fires, then lets the in-flight job finish and
    /// stops every module's [`Lifecycle`].
    ///
    /// # Errors
    ///
    /// When the HTTP server fails or the worker task panics.
    pub async fn serve(
        self,
        router: Router,
        listener: tokio::net::TcpListener,
        shutdown: CancellationToken,
    ) -> anyhow::Result<()> {
        let worker = self.worker.clone();
        let worker_shutdown = shutdown.clone();
        let worker_task = tokio::spawn(async move { worker.run(worker_shutdown).await });
        let mcp_shutdown = self.shutdown.clone();
        let server_shutdown = shutdown.clone();
        let served =
            axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async move {
                    server_shutdown.cancelled().await;
                    mcp_shutdown.cancel();
                })
                .await
                .context("serving HTTP");
        shutdown.cancel();
        // Modules stop and the database closes even when the worker panicked,
        // so buffered work is never left behind.
        let joined = worker_task.await.context("joining the worker");
        self.platform.stop_modules().await;
        self.platform.db().close().await;
        served.and(joined)
    }
}

async fn not_found() -> Response {
    ApiError::NotFound.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_typed_spec_loses_nothing() {
        let json = spec_json(&register_all(&[], None).unwrap()).unwrap();
        let typed = serde_json::to_value(openapi(&[]).unwrap()).unwrap();
        assert_eq!(typed, json, "the utoipa OpenApi round trip keeps every field");
    }

    #[test]
    fn module_docs_nest_under_the_module() {
        let docs =
            serde_json::json!({ "paths": { "/": { "post": {} }, "/submit": { "post": {} } } });
        let nested = nest_docs("/m/forms", &docs);
        let paths: Vec<&String> = nested["paths"].as_object().unwrap().keys().collect();
        assert_eq!(paths, ["/m/forms", "/m/forms/submit"], "nested paths");
    }
}
