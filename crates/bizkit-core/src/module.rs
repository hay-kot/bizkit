//! Feature modules plug in through [`Module`]. Core's own auth and tenancy
//! features register through it too.

use std::collections::BTreeMap;
use std::fmt;

use anyhow::{Context as _, bail};
use serde::de::DeserializeOwned;
use sqlx::migrate::Migrator;

use crate::jobs::{Event, Handlers, Job, JobHandler, Subscriber};
use crate::ops::{Access, Operation, Registry};

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
}

/// What modules register into.
#[derive(Debug, Default)]
pub(crate) struct Registration {
    pub(crate) registry: Registry,
    pub(crate) handlers: Handlers,
    pub(crate) routes: Vec<ModuleRoutes>,
}

/// A module's raw routes and their docs.
#[derive(Debug)]
pub(crate) struct ModuleRoutes {
    pub(crate) module: &'static str,
    pub(crate) router: axum::Router,
    pub(crate) docs: serde_json::Value,
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
    ///
    /// # Errors
    ///
    /// When the module already registered routes.
    pub fn routes(
        &mut self,
        router: axum::Router,
        docs: utoipa::openapi::OpenApi,
    ) -> anyhow::Result<()> {
        if self.registration.routes.iter().any(|routes| routes.module == self.module) {
            bail!("module {} registered routes twice", self.module);
        }
        let docs = serde_json::to_value(docs).context("serializing route docs")?;
        self.registration.routes.push(ModuleRoutes { module: self.module, router, docs });
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
        self.registration.handlers.add_subscriber::<E, S>(subscriber)
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
