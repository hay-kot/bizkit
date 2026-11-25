//! Feature modules plug in through [`Module`]. Core's own auth and tenancy
//! features register through it too.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, bail};
use serde::de::DeserializeOwned;
use sqlx::migrate::Migrator;

use futures::future::BoxFuture;

use crate::app::Platform;
use crate::db::Db;
use crate::hooks::{
    HOOKS_SUBSCRIBER, HookKind, HookPoint, HookRegistry, HookRunner, HookSource, MAX_SYNC_BUDGET,
    TargetKind, TargetResolver,
};
use crate::ids::{ProjectId, UserId, WorkspaceId};
use crate::jobs::{Event, Handlers, Job, JobHandler, Subscriber};
use crate::ops::{Access, Operation, Registry};
use crate::time::UnixMillis;

/// A feature module: its migrations, operations, routes, jobs and
/// subscribers.
pub trait Module: Send + Sync + fmt::Debug + 'static {
    /// The module name: its config key, route nest (`/m/<name>`) and the
    /// prefix of its scope areas and job kinds.
    fn name(&self) -> &'static str;

    /// The module's migrations, usually `sqlx::migrate!()`. Versions must be
    /// unique across core and every module.
    fn migrations(&self) -> Option<Migrator> {
        None
    }

    /// Declares everything the module provides. It must not do I/O, and must
    /// register the same operations whatever the config says, so the spec is
    /// the same on every install. Operations and handlers reach the database
    /// at call time through `OpCtx::platform()` and `JobCtx::platform`.
    ///
    /// # Errors
    ///
    /// Anything that should stop the app from starting, such as a registration conflict.
    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()>;

    /// Adds sample rows for the dev stack on top of the demo workspace. It
    /// must be idempotent: a second run changes nothing. Only the
    /// `test-hooks` dev command calls it.
    ///
    /// # Errors
    ///
    /// When a database write fails.
    fn seed_demo<'a>(
        &'a self,
        _db: &'a Db,
        _seed: &'a mut DemoSeed,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async { Ok(()) })
    }

    /// Runs once as the app starts, after migrations and before it serves,
    /// for data upgrades that need what migrations can't see, such as the
    /// secrets key. It must be idempotent.
    ///
    /// # Errors
    ///
    /// Anything that should stop the app from starting.
    fn startup<'a>(&'a self, _platform: &'a Platform) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async { Ok(()) })
    }
}

/// Background state a module keeps across requests, such as write buffers.
/// Register it with [`ModuleContext::lifecycle`].
pub trait Lifecycle: Send + Sync + fmt::Debug + 'static {
    /// Writes everything buffered so far, side effects included. The module
    /// keeps working afterwards.
    ///
    /// # Errors
    ///
    /// When buffered work can't be written.
    fn flush(&self) -> BoxFuture<'_, anyhow::Result<()>>;

    /// Flushes and stops. Runs once, after HTTP and the worker stopped and
    /// before the database closes.
    fn stop(&self) -> BoxFuture<'_, ()>;
}

/// The demo rows core seeded, for [`Module::seed_demo`].
#[derive(Debug, Clone)]
pub struct DemoSeed {
    /// The demo workspace.
    pub workspace_id: WorkspaceId,
    /// Its projects by slug: `website`, `docs` and `bizkit`.
    pub project_ids: BTreeMap<String, ProjectId>,
    /// The demo user, an admin of the workspace.
    pub user_id: UserId,
    /// The seed time.
    pub now: UnixMillis,
    /// The data directory, for modules that keep files beside the database.
    pub data_dir: PathBuf,
    /// Entities modules seeded, by (target kind ID, label), such as
    /// `("forms.form", "Contact")`, so later modules can refer to them
    /// without depending on the module that made them.
    pub entities: BTreeMap<(String, String), String>,
}

/// What modules register into.
#[derive(Debug, Default)]
pub(crate) struct Registration {
    pub(crate) registry: Registry,
    pub(crate) handlers: Handlers,
    pub(crate) routes: Vec<ModuleRoutes>,
    pub(crate) hooks: HookRegistry,
    pub(crate) lifecycles: Vec<Arc<dyn Lifecycle>>,
    lifecycle_modules: Vec<&'static str>,
}

/// The largest file an upload route accepts.
///
/// The body limit is this plus 128 KiB for multipart framing and small
/// parts beside the file (such as a JSON part of up to 64 KiB); handlers
/// check the file part itself.
pub const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

/// Where a module's raw routes sit in the router.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteKind {
    /// Inside the authenticate layer, with the 1 MiB body limit.
    Authenticated,
    /// Outside the authenticate layer.
    Public,
    /// Inside the authenticate layer, with the upload body limit.
    Upload,
}

impl RouteKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Authenticated => "routes",
            Self::Public => "public routes",
            Self::Upload => "upload routes",
        }
    }
}

/// A module's raw routes and their docs.
#[derive(Debug)]
pub(crate) struct ModuleRoutes {
    pub(crate) module: &'static str,
    pub(crate) kind: RouteKind,
    pub(crate) router: axum::Router,
    pub(crate) docs: serde_json::Value,
}

impl ModuleRoutes {
    /// Where the router nests: `/m/<module>`, `/m/<module>/public` or
    /// `/m/<module>/upload`.
    pub(crate) fn nest(&self) -> String {
        match self.kind {
            RouteKind::Authenticated => format!("/m/{}", self.module),
            RouteKind::Public => format!("/m/{}/public", self.module),
            RouteKind::Upload => format!("/m/{}/upload", self.module),
        }
    }
}

/// The registration API a module's [`Module::register`] gets.
#[derive(Debug)]
pub struct ModuleContext<'a> {
    module: &'static str,
    /// Core may declare areas and job kinds outside its own name.
    is_core: bool,
    /// The module's config section; `None` in a spec-only context.
    config: Option<&'a serde_json::Value>,
    registration: &'a mut Registration,
}

impl<'a> ModuleContext<'a> {
    pub(crate) const fn new(
        module: &'static str,
        is_core: bool,
        config: Option<&'a serde_json::Value>,
        registration: &'a mut Registration,
    ) -> Self {
        Self { module, is_core, config, registration }
    }

    /// The module's `modules.<name>` config section. Absent sections and the
    /// spec-only context (`bizkit openapi`) give `T::default()`.
    ///
    /// # Errors
    ///
    /// When the section doesn't deserialize into `T`.
    pub fn config<T: DeserializeOwned + Default>(&self) -> anyhow::Result<T> {
        match self.config {
            None | Some(serde_json::Value::Null) => Ok(T::default()),
            Some(value) => serde_json::from_value(value.clone())
                .with_context(|| format!("reading config section modules.{}", self.module)),
        }
    }

    fn owns(&self, name: &str) -> bool {
        self.is_core
            || name == self.module
            || name.strip_prefix(self.module).is_some_and(|rest| rest.starts_with(['.', '_', '-']))
    }

    /// Declares a scope area. Module areas are the module name or start with
    /// it (`forms`, `forms.responses`).
    ///
    /// # Errors
    ///
    /// When the area is already declared or doesn't start with the module name.
    pub fn scope_area(
        &mut self,
        area: &'static str,
        description: &'static str,
    ) -> anyhow::Result<()> {
        if !self.owns(area) {
            bail!(
                "module {} can't declare scope area {area}: areas must start with the module name",
                self.module
            );
        }
        self.registration.registry.declare_area(area, description)
    }

    /// Registers an operation. Module paths start with
    /// `/workspaces/{workspace_id}/<module>` or `/<module>`.
    ///
    /// # Errors
    ///
    /// When the path is outside the module's prefix, or the registry rejects the operation.
    pub fn op<O: Operation>(&mut self, op: O) -> anyhow::Result<()> {
        if !self.is_core {
            let scoped = format!("/workspaces/{{workspace_id}}/{}", self.module);
            let top = format!("/{}", self.module);
            let under =
                |prefix: &str| O::PATH == prefix || O::PATH.starts_with(&format!("{prefix}/"));
            if !under(&scoped) && !under(&top) {
                bail!(
                    "operation {}: module {} paths must start with {scoped} or {top}",
                    O::ID,
                    self.module
                );
            }
            if let Access::Workspace(scope) = &O::ACCESS
                && !self.registration.registry.areas().contains(scope.area())
            {
                bail!("operation {}: scope area {} is not declared", O::ID, scope.area());
            }
        }
        self.registration.registry.add(op)
    }

    /// Adds raw routes, nested at `/m/<module>`. `docs` describes them with
    /// paths relative to the nest. Handlers reach the platform through
    /// `Extension<Platform>` and the caller through `Extension<Authn>`.
    /// `/public` and `/upload` under the nest are reserved for
    /// [`Self::public_routes`] and [`Self::upload_routes`], so these routes
    /// must not use them.
    ///
    /// # Errors
    ///
    /// When the module already registered routes.
    pub fn routes(
        &mut self,
        router: axum::Router,
        docs: utoipa::openapi::OpenApi,
    ) -> anyhow::Result<()> {
        self.add_routes(RouteKind::Authenticated, router, docs)
    }

    /// Adds routes that third-party pages call, nested at
    /// `/m/<module>/public`, such as form ingest. They sit outside the
    /// authenticate layer: handlers get no `Extension<Authn>`, cookies and
    /// bearer tokens are never read, and the cookie CSRF check never
    /// applies. `docs` paths are relative to the nest. Handlers reach the
    /// platform through `Extension<Platform>`. The `/m/<module>/public`
    /// prefix belongs to these routes alone.
    ///
    /// # Errors
    ///
    /// When the module already registered public routes.
    pub fn public_routes(
        &mut self,
        router: axum::Router,
        docs: utoipa::openapi::OpenApi,
    ) -> anyhow::Result<()> {
        self.add_routes(RouteKind::Public, router, docs)
    }

    /// Adds authenticated routes that take large bodies, such as file
    /// uploads, nested at `/m/<module>/upload`. They sit inside the
    /// authenticate layer like [`Self::routes`] (`Extension<Authn>`, the
    /// cookie CSRF check), with a body limit of [`MAX_UPLOAD_BYTES`] plus
    /// 128 KiB and a 300 s timeout, instead of 1 MiB and 30 s. The limit
    /// covers every extractor (axum's `DefaultBodyLimit` is off there), and
    /// a body over it gets 413. Read each file part with
    /// [`crate::upload::read_file`], which holds the file itself to
    /// [`MAX_UPLOAD_BYTES`]. Check access with
    /// [`crate::ops::authorize_workspace`]. `docs` paths are relative to the
    /// nest. The `/m/<module>/upload` prefix belongs to these routes alone.
    ///
    /// # Errors
    ///
    /// When the module already registered upload routes.
    pub fn upload_routes(
        &mut self,
        router: axum::Router,
        docs: utoipa::openapi::OpenApi,
    ) -> anyhow::Result<()> {
        self.add_routes(RouteKind::Upload, router, docs)
    }

    fn add_routes(
        &mut self,
        kind: RouteKind,
        router: axum::Router,
        docs: utoipa::openapi::OpenApi,
    ) -> anyhow::Result<()> {
        let module = self.module;
        if self.registration.routes.iter().any(|r| r.module == module && r.kind == kind) {
            bail!("module {module} registered {} twice", kind.label());
        }
        let docs = serde_json::to_value(docs).context("serializing route docs")?;
        self.registration.routes.push(ModuleRoutes { module, kind, router, docs });
        Ok(())
    }

    /// Registers the handler for job kind `J::KIND`, which must start with the
    /// module name.
    ///
    /// # Errors
    ///
    /// When the kind doesn't start with the module name or is already registered.
    pub fn job<J: Job, H: JobHandler<J>>(&mut self, handler: H) -> anyhow::Result<()> {
        if !self.owns(J::KIND) {
            bail!(
                "module {} can't handle job kind {}: kinds must start with the module name",
                self.module,
                J::KIND
            );
        }
        self.registration.handlers.add_job::<J, H>(handler)
    }

    /// Subscribes to event `E`. Each published event becomes one job per
    /// subscriber.
    ///
    /// # Errors
    ///
    /// When the subscriber is already registered for the event.
    pub fn subscribe<E: Event, S: Subscriber<E>>(&mut self, subscriber: S) -> anyhow::Result<()> {
        if !self.is_core && S::NAME == HOOKS_SUBSCRIBER {
            bail!(
                "module {} can't subscribe to {} as {HOOKS_SUBSCRIBER}: core reserves that name",
                self.module,
                E::NAME
            );
        }
        self.registration.handlers.add_subscriber::<E, S>(subscriber)
    }

    /// Declares a sync hook point. Its ID starts with the module name, and
    /// its budget is above zero and at most [`MAX_SYNC_BUDGET`].
    ///
    /// # Errors
    ///
    /// When the point is async, its ID is taken or outside the module, or its budget is out of range.
    pub fn sync_hook(&mut self, point: HookPoint) -> anyhow::Result<()> {
        let HookKind::Sync { budget, .. } = point.kind else {
            bail!("hook point {} is async; declare it with async_hook", point.id);
        };
        if budget.is_zero() || budget > MAX_SYNC_BUDGET {
            bail!(
                "hook point {} has budget {budget:?}; it must be above zero and at most {MAX_SYNC_BUDGET:?}",
                point.id
            );
        }
        self.check_point(&point)?;
        self.registration.hooks.add_point(point);
        Ok(())
    }

    /// Declares an async hook point on event `E`. Core subscribes to `E` and,
    /// for each event, asks `source` for the target and hands the event to
    /// the runner. Runs rebuild their input through `source` too.
    ///
    /// # Errors
    ///
    /// When the point is sync, its event isn't `E`, `E` already has an async point, or its ID is taken or outside the module.
    pub fn async_hook<E: Event, S: HookSource<E>>(
        &mut self,
        point: HookPoint,
        source: S,
    ) -> anyhow::Result<()> {
        let HookKind::Async { event } = point.kind else {
            bail!("hook point {} is sync; declare it with sync_hook", point.id);
        };
        if event != E::NAME {
            bail!("hook point {} names event {event} but subscribes to {}", point.id, E::NAME);
        }
        if let Some(other) = self.registration.hooks.async_point_for(event) {
            bail!("event {event} already has async hook point {}", other.id);
        }
        self.check_point(&point)?;
        let fanout = self.registration.hooks.add_source::<E, S>(point, source);
        self.registration.handlers.add_subscriber::<E, _>(fanout)?;
        self.registration.hooks.add_point(point);
        Ok(())
    }

    fn check_point(&self, point: &HookPoint) -> anyhow::Result<()> {
        if !self.owns(point.id) {
            bail!(
                "module {} can't declare hook point {}: IDs must start with the module name",
                self.module,
                point.id
            );
        }
        if self.registration.hooks.has_point(point.id) {
            bail!("hook point {} is already declared", point.id);
        }
        if let Some(kind) = point.target
            && !self.registration.hooks.has_kind(kind.id)
        {
            bail!(
                "hook point {} targets {}, which isn't declared yet; call hook_target_kind first",
                point.id,
                kind.id
            );
        }
        Ok(())
    }

    /// Declares a kind of entity that scripts on this module's hook points
    /// can bind to. Declare it before the points that target it.
    ///
    /// # Errors
    ///
    /// When the kind's ID is taken or doesn't start with the module name.
    pub fn hook_target_kind(
        &mut self,
        kind: &'static TargetKind,
        resolver: impl TargetResolver,
    ) -> anyhow::Result<()> {
        if !self.owns(kind.id) {
            bail!(
                "module {} can't declare target kind {}: IDs must start with the module name",
                self.module,
                kind.id
            );
        }
        if self.registration.hooks.has_kind(kind.id) {
            bail!("target kind {} is already declared", kind.id);
        }
        self.registration.hooks.add_kind(kind, Arc::new(resolver));
        Ok(())
    }

    /// Registers the module's [`Lifecycle`]: the app flushes it on demand
    /// and stops it on shutdown.
    ///
    /// # Errors
    ///
    /// When the module already registered one.
    pub fn lifecycle(&mut self, lifecycle: impl Lifecycle) -> anyhow::Result<()> {
        let module = self.module;
        if self.registration.lifecycle_modules.contains(&module) {
            bail!("module {module} registered a lifecycle twice");
        }
        self.registration.lifecycle_modules.push(module);
        self.registration.lifecycles.push(Arc::new(lifecycle));
        Ok(())
    }

    /// Provides the runner that runs scripts for every hook point. Only one
    /// module may.
    ///
    /// # Errors
    ///
    /// When another module already provided one.
    pub fn hook_runner(&mut self, runner: impl HookRunner) -> anyhow::Result<()> {
        if self.registration.hooks.has_runner() {
            bail!("module {} provides a hook runner, but one is already registered", self.module);
        }
        self.registration.hooks.set_runner(Arc::new(runner));
        Ok(())
    }
}

/// Registers core and then `modules` in order.
pub(crate) fn register_all(
    modules: &[Box<dyn Module>],
    configs: Option<&BTreeMap<String, serde_json::Value>>,
) -> anyhow::Result<Registration> {
    let mut registration = Registration::default();
    let core = crate::core_module::CoreModule;
    core.register(&mut ModuleContext::new(core.name(), true, None, &mut registration))
        .context("registering core")?;
    let mut seen = vec![core.name()];
    for module in modules {
        let name = module.name();
        if seen.contains(&name) {
            bail!("module {name} is listed twice");
        }
        seen.push(name);
        // In a spec-only build (`configs` is None) every module sees defaults.
        let config = configs.map(|configs| configs.get(name).unwrap_or(&serde_json::Value::Null));
        module
            .register(&mut ModuleContext::new(name, false, config, &mut registration))
            .with_context(|| format!("registering module {name}"))?;
    }
    Ok(registration)
}
