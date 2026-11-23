//! A real app on a temp-dir database, with fakes for every side effect.

#![expect(clippy::panic, reason = "test setup that fails should fail the test")]

use std::path::Path;
use std::sync::Arc;

use axum_test::TestServer;
use bizkit_core::config::PolicyConfig;
use bizkit_core::deps::Clock as _;
use bizkit_core::dev::personas::{self, Manifest, Persona, SeedOptions};
use bizkit_core::ids::WorkspaceId;
use bizkit_core::jobs::RunStats;
use bizkit_core::{App, Config, Deps, Module, ModuleContext};
use http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::fakes::{FakeClock, FakeHooks, FakeHttp, FakeMailer, MemBlobs};
use crate::mcp_client::McpClient;
use crate::spec_client::{SpecChecker, SpecClient, session_from};

/// The base URL test apps run under.
pub const BASE_URL: &str = "http://localhost:8484";

/// A running test app. The background worker is off: call
/// [`TestApp::drain_jobs`] so tests stay deterministic.
#[derive(Debug)]
pub struct TestApp {
    /// The app's clock.
    pub clock: FakeClock,
    /// The app's mailer.
    pub mail: FakeMailer,
    /// The app's outbound HTTP.
    pub http: FakeHttp,
    /// The app's blob store.
    pub blobs: MemBlobs,
    app: App,
    server: Arc<TestServer>,
    checker: Arc<SpecChecker>,
    dir: TempDir,
}

#[derive(Debug)]
struct FakeHooksModule(FakeHooks);

impl Module for FakeHooksModule {
    fn name(&self) -> &'static str {
        "fake_hooks"
    }

    fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
        cx.hook_runner(self.0.clone())
    }
}

/// Configures a [`TestApp`].
#[derive(Debug)]
pub struct TestAppBuilder {
    config: Config,
    modules: Vec<Box<dyn Module>>,
}

impl TestAppBuilder {
    /// Changes the config before the app builds.
    #[must_use]
    pub fn config(mut self, f: impl FnOnce(&mut Config)) -> Self {
        f(&mut self.config);
        self
    }

    /// Adds a module after core.
    #[must_use]
    pub fn module(mut self, module: impl Module) -> Self {
        self.modules.push(Box::new(module));
        self
    }

    /// Adds an already boxed module after core.
    #[must_use]
    pub fn boxed_module(mut self, module: Box<dyn Module>) -> Self {
        self.modules.push(module);
        self
    }

    /// Runs hook points through `hooks` instead of the inert default. It
    /// registers a small module, `fake_hooks`, that provides the runner.
    #[must_use]
    pub fn hook_runner(self, hooks: FakeHooks) -> Self {
        self.module(FakeHooksModule(hooks))
    }

    /// Builds and starts the app.
    ///
    /// # Panics
    ///
    /// When the app fails to build.
    pub async fn build(self) -> TestApp {
        let dir = tempfile::tempdir().unwrap_or_else(|err| panic!("creating a temp dir: {err}"));
        let mut config = self.config;
        config.data_dir = Some(dir.path().to_path_buf());
        let clock = FakeClock::default();
        let mail = FakeMailer::default();
        let http = FakeHttp::default();
        let blobs = MemBlobs::default();
        let deps = Deps {
            clock: Arc::new(clock.clone()),
            mailer: Arc::new(mail.clone()),
            http: Arc::new(http.clone()),
            blobs: Arc::new(blobs.clone()),
        };
        let app = App::build(config, deps, self.modules)
            .await
            .unwrap_or_else(|err| panic!("building the app: {err:#}"));
        let checker = Arc::new(SpecChecker::new(app.openapi_json().clone()));
        let server = Arc::new(TestServer::new(app.router()));
        TestApp { clock, mail, http, blobs, app, server, checker, dir }
    }
}

impl TestApp {
    /// An app with core only and the default test config: one `sign_up`
    /// policy, `test`, that admits `*@example.com`.
    ///
    /// # Panics
    ///
    /// When the app fails to build.
    pub async fn new() -> Self {
        Self::builder().build().await
    }

    /// A builder with the default test config.
    ///
    /// # Panics
    ///
    /// Never: the base URL is a valid constant.
    #[must_use]
    pub fn builder() -> TestAppBuilder {
        let mut config = Config {
            base_url: Some(BASE_URL.parse().unwrap_or_else(|err| panic!("{err}"))),
            ..Config::default()
        };
        config.auth.policies = vec![PolicyConfig {
            name: "test".to_owned(),
            matches: vec!["*@example.com".to_owned()],
            sign_up: true,
            join: Vec::new(),
        }];
        TestAppBuilder { config, modules: Vec::new() }
    }

    /// The app.
    #[must_use]
    pub const fn app(&self) -> &App {
        &self.app
    }

    /// The temp data dir.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        self.dir.path()
    }

    fn client_with(&self, session: Option<String>, bearer: Option<String>) -> SpecClient {
        SpecClient::new(
            Arc::clone(&self.server),
            Arc::clone(&self.checker),
            session,
            bearer,
            BASE_URL.to_owned(),
        )
    }

    /// An anonymous client.
    #[must_use]
    pub fn client(&self) -> SpecClient {
        self.client_with(None, None)
    }

    /// A client with a session token, as a seeded persona has.
    #[must_use]
    pub fn with_session(&self, token: &str) -> SpecClient {
        self.client_with(Some(token.to_owned()), None)
    }

    /// A client that sends `token` as a bearer API key.
    #[must_use]
    pub fn with_key(&self, token: &str) -> SpecClient {
        self.client_with(None, Some(token.to_owned()))
    }

    /// Signs in for real: requests a link, reads it from the fake mailer and
    /// verifies it.
    ///
    /// # Panics
    ///
    /// When no link arrives (the email isn't admitted, or the 60 s cooldown
    /// applies) or verifying fails.
    pub async fn sign_in(&self, email: &str) -> SpecClient {
        let token = self.request_link(email).await.unwrap_or_else(|| {
            panic!("no sign-in link went to {email}: no policy or invitation admits it, or the cooldown applies")
        });
        self.verify(&token)
            .await
            .unwrap_or_else(|status| panic!("verifying the link for {email} gave {status}"))
    }

    /// Requests a sign-in link and returns its token, or `None` when no mail
    /// went out.
    ///
    /// # Panics
    ///
    /// When the request doesn't answer 202.
    pub async fn request_link(&self, email: &str) -> Option<String> {
        let before = self.mail.sent().len();
        self.client()
            .post("/api/auth/sign-in", &json!({ "email": email }))
            .await
            .expect(StatusCode::ACCEPTED);
        self.drain_jobs().await;
        let sent = self.mail.sent();
        let to = bizkit_core::auth::normalize_email(email);
        let mail = sent.get(before..)?.iter().rev().find(|mail| mail.to == to)?;
        link_token(&mail.text)
    }

    /// Verifies a link token.
    ///
    /// # Errors
    ///
    /// The response status when verifying fails.
    pub async fn verify(&self, token: &str) -> Result<SpecClient, StatusCode> {
        let response = self.client().post("/api/auth/verify", &json!({ "token": token })).await;
        if response.status != StatusCode::NO_CONTENT {
            return Err(response.status);
        }
        let session = session_from(&response.headers).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(self.with_session(&session))
    }

    /// Seeds `persona` under `namespace`.
    ///
    /// # Panics
    ///
    /// When seeding fails.
    pub async fn seed(&self, persona: Persona, namespace: &str) -> Manifest {
        let platform = self.app.platform();
        let options = SeedOptions {
            areas: platform.scope_areas(),
            now: self.clock.now(),
            session_ttl: platform.session_ttl(),
        };
        personas::seed(platform.db(), &options, persona, namespace)
            .await
            .unwrap_or_else(|err| panic!("seeding {persona}: {err:#}"))
    }

    /// Writes an API key with exactly `scopes` (even none, which the API
    /// refuses) straight into the database and returns its token.
    ///
    /// # Panics
    ///
    /// When the insert fails.
    pub async fn insert_api_key(&self, workspace_id: WorkspaceId, scopes: &[&str]) -> String {
        let token = format!("bk_{}", bizkit_core::auth::random_token());
        let scopes = serde_json::to_string(scopes).unwrap_or_else(|err| panic!("{err}"));
        let now = self.clock.now();
        let mut tx = self.app.platform().db().write().await.unwrap_or_else(|err| panic!("{err:#}"));
        sqlx::query(
            "INSERT INTO api_keys (id, workspace_id, name, prefix, secret_hash, scopes, created_at)
             VALUES (?, ?, 'test', ?, ?, ?, ?)",
        )
        .bind(bizkit_core::ids::ApiKeyId::new())
        .bind(workspace_id)
        .bind(token.chars().take(10).collect::<String>())
        .bind(bizkit_core::auth::hash_token(&token))
        .bind(scopes)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap_or_else(|err| panic!("inserting an API key: {err}"));
        tx.commit().await.unwrap_or_else(|err| panic!("{err:#}"));
        token
    }

    /// Runs every job due now.
    ///
    /// # Panics
    ///
    /// When the worker can't claim or finish a job.
    pub async fn drain_jobs(&self) -> RunStats {
        self.app
            .worker()
            .run_until_idle()
            .await
            .unwrap_or_else(|err| panic!("draining jobs: {err:#}"))
    }

    /// An MCP client with the API key `token`.
    #[must_use]
    pub fn mcp(&self, token: &str) -> McpClient {
        McpClient::new(Arc::clone(&self.server), Some(token.to_owned()))
    }

    /// An MCP client with no credentials.
    #[must_use]
    pub fn mcp_anonymous(&self) -> McpClient {
        McpClient::new(Arc::clone(&self.server), None)
    }
}

/// The `token` query parameter of the first sign-in link in `text`.
#[must_use]
pub fn link_token(text: &str) -> Option<String> {
    let url = text.split_whitespace().find(|word| word.contains("/sign-in/verify?"))?;
    let url = url::Url::parse(url).ok()?;
    url.query_pairs().find(|(k, _)| k == "token").map(|(_, v)| v.into_owned())
}
