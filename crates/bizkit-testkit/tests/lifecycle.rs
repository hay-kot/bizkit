//! Module lifecycles: flushes on demand and before jobs, a stop on shutdown,
//! and the project deletion event.

#[cfg(test)]
mod lifecycle {
    use std::sync::{Arc, Mutex};

    use bizkit_core::api::ProjectDeleted;
    use bizkit_core::jobs::{JobCtx, Subscriber};
    use bizkit_core::{Lifecycle, Module, ModuleContext};
    use bizkit_testkit::{Persona, TestApp};
    use futures::future::BoxFuture;
    use http::StatusCode;

    /// What the test module's lifecycle and subscriber saw, in order.
    #[derive(Debug, Clone, Default)]
    struct Log(Arc<Mutex<Vec<String>>>);

    impl Log {
        fn push(&self, entry: impl Into<String>) {
            self.0.lock().unwrap().push(entry.into());
        }

        fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    #[derive(Debug)]
    struct Recorder(Log);

    impl Lifecycle for Recorder {
        fn flush(&self) -> BoxFuture<'_, anyhow::Result<()>> {
            self.0.push("flush");
            Box::pin(async { Ok(()) })
        }

        fn stop(&self) -> BoxFuture<'_, ()> {
            self.0.push("stop");
            Box::pin(async {})
        }
    }

    struct OnDeleted(Log);

    impl Subscriber<ProjectDeleted> for OnDeleted {
        const NAME: &'static str = "lifetest";

        fn handle(
            &self,
            _cx: JobCtx,
            event: ProjectDeleted,
        ) -> impl Future<Output = anyhow::Result<()>> + Send {
            self.0.push(format!("deleted {}", event.project_id));
            std::future::ready(Ok(()))
        }
    }

    #[derive(Debug, Default)]
    struct LifeTest {
        log: Log,
        twice: bool,
    }

    impl Module for LifeTest {
        fn name(&self) -> &'static str {
            "lifetest"
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            cx.lifecycle(Recorder(self.log.clone()))?;
            if self.twice {
                cx.lifecycle(Recorder(self.log.clone()))?;
            }
            cx.subscribe::<ProjectDeleted, _>(OnDeleted(self.log.clone()))
        }
    }

    async fn app(log: &Log) -> TestApp {
        TestApp::builder().module(LifeTest { log: log.clone(), twice: false }).build().await
    }

    #[tokio::test]
    async fn flush_and_drain_flush_every_module() {
        let log = Log::default();
        let app = app(&log).await;
        app.flush().await;
        assert_eq!(log.take(), ["flush"], "an explicit flush");
        app.drain_jobs().await;
        assert_eq!(log.take(), ["flush"], "draining jobs flushes first");
    }

    #[tokio::test]
    async fn shutdown_stops_every_module_once() {
        let log = Log::default();
        let app = app(&log).await;
        app.shut_down().await;
        assert_eq!(log.take(), ["stop"], "stopped once, after serving");
    }

    #[tokio::test]
    async fn a_module_registers_one_lifecycle() {
        let module = LifeTest { log: Log::default(), twice: true };
        let modules: Vec<Box<dyn Module>> = vec![Box::new(module)];
        let Err(err) = bizkit_core::openapi(&modules) else { panic!("registered twice") };
        assert!(
            format!("{err:#}").contains("module lifetest registered a lifecycle twice"),
            "{err:#}"
        );
    }

    #[tokio::test]
    async fn deleting_a_project_publishes_an_event() {
        let log = Log::default();
        let app = app(&log).await;
        let manifest = app.seed(Persona::Team, "life").await;
        let admin = app.with_session(&manifest.accounts["admin"].session_token);
        let workspace = manifest.workspaces["main"].id;
        let project = manifest.projects.values().next().expect("a seeded project").id;
        let path = format!("/api/v1/workspaces/{workspace}/projects/{project}");
        admin.delete(&path).await.expect(StatusCode::NO_CONTENT);
        app.drain_jobs().await;
        assert_eq!(log.take(), ["flush", format!("deleted {project}").as_str()], "subscriber ran");

        admin.delete(&path).await.expect(StatusCode::NOT_FOUND);
        app.drain_jobs().await;
        assert_eq!(log.take(), ["flush"], "a missing project publishes nothing");
    }
}
