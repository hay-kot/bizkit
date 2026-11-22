//! The worker: retry, backoff, dead-letter, panics, lanes and timeouts.

#[cfg(test)]
mod jobs {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use bizkit_core::jobs::{Enqueue, Job, JobCtx, JobHandler, Lane};
    use bizkit_core::{Module, ModuleContext};
    use bizkit_testkit::TestApp;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct Flaky {
        fail_times: u32,
        panic: bool,
    }

    impl Job for Flaky {
        const KIND: &'static str = "jobtest.flaky";
        const MAX_ATTEMPTS: u32 = 3;
    }

    #[derive(Debug, Clone, Default)]
    struct FlakyHandler {
        calls: Arc<AtomicU32>,
    }

    impl JobHandler<Flaky> for FlakyHandler {
        fn handle(
            &self,
            _cx: JobCtx,
            job: Flaky,
        ) -> impl Future<Output = anyhow::Result<()>> + Send {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            assert!(!job.panic, "handler told to panic");
            let result = if call <= job.fail_times {
                Err(anyhow::anyhow!("failure {call}"))
            } else {
                Ok(())
            };
            std::future::ready(result)
        }
    }

    #[derive(Debug, Clone, Default)]
    struct JobTest {
        handler: FlakyHandler,
    }

    impl Module for JobTest {
        fn name(&self) -> &'static str {
            "jobtest"
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            cx.job(self.handler.clone())
        }
    }

    async fn app_with_job(job: Flaky) -> (TestApp, Arc<AtomicU32>) {
        let module = JobTest::default();
        let calls = Arc::clone(&module.handler.calls);
        let app = TestApp::builder().module(module).build().await;
        let platform = app.app().platform();
        let mut tx = platform.db().write().await.unwrap();
        platform.bus().enqueue(&mut tx, &job, Enqueue::default()).await.unwrap();
        tx.commit().await.unwrap();
        (app, calls)
    }

    async fn job_row(app: &TestApp) -> (String, i64, Option<String>) {
        sqlx::query_as("SELECT status, attempts, last_error FROM jobs WHERE kind = 'jobtest.flaky'")
            .fetch_one(app.app().platform().db().read())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn failures_retry_with_backoff() {
        let (app, calls) = app_with_job(Flaky { fail_times: 1, panic: false }).await;
        let first = app.drain_jobs().await;
        assert_eq!((first.done, first.retried, first.dead), (0, 1, 0), "first attempt fails");
        assert_eq!(app.drain_jobs().await.retried, 0, "not due again yet");

        app.clock.advance(Duration::from_secs(2));
        let second = app.drain_jobs().await;
        assert_eq!(second.done, 1, "second attempt succeeds after 2 s");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "two calls");
        assert_eq!(job_row(&app).await.0, "done", "done");
    }

    #[tokio::test]
    async fn jobs_go_dead_after_max_attempts() {
        let (app, _) = app_with_job(Flaky { fail_times: 10, panic: false }).await;
        for _ in 0..3 {
            app.drain_jobs().await;
            app.clock.advance(Duration::from_mins(1));
        }
        let (status, attempts, last_error) = job_row(&app).await;
        assert_eq!((status.as_str(), attempts), ("dead", 3), "dead after three attempts");
        assert_eq!(last_error.as_deref(), Some("failure 3"), "keeps the last error");
    }

    #[tokio::test]
    async fn a_panicking_handler_counts_as_a_failure() {
        let (app, _) = app_with_job(Flaky { fail_times: 0, panic: true }).await;
        let run = app.drain_jobs().await;
        assert_eq!(run.retried, 1, "the panic was caught and retried");
        let (status, _, last_error) = job_row(&app).await;
        assert_eq!(status, "queued", "requeued");
        assert!(last_error.unwrap().contains("handler panicked"), "panic recorded");
    }

    #[tokio::test]
    async fn mail_failures_retry() {
        let app = TestApp::new().await;
        app.mail.fail_next(1);
        app.client()
            .post("/api/auth/sign-in", &serde_json::json!({ "email": "a@example.com" }))
            .await
            .expect(http::StatusCode::ACCEPTED);
        assert_eq!(app.drain_jobs().await.retried, 1, "first send fails");
        app.clock.advance(Duration::from_secs(2));
        assert_eq!(app.drain_jobs().await.done, 1, "retry sends");
        assert!(app.mail.last_to("a@example.com").is_some(), "mail went out");
    }

    #[tokio::test]
    async fn enqueueing_an_unregistered_kind_fails() {
        let app = TestApp::new().await;
        let platform = app.app().platform();
        let mut tx = platform.db().write().await.unwrap();
        let enqueued = platform
            .bus()
            .enqueue(&mut tx, &Flaky { fail_times: 0, panic: false }, Enqueue::default())
            .await;
        assert!(enqueued.is_err(), "no handler registered");
    }

    /// Waits in its own lane until the test releases it.
    #[derive(Debug, Serialize, Deserialize)]
    struct Blocker;

    impl Job for Blocker {
        const KIND: &'static str = "lanetest.block";
        const LANE: Lane = Lane { name: "lanetest", concurrency: 1 };
    }

    #[derive(Debug, Clone, Default)]
    struct BlockerHandler {
        started: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }

    impl JobHandler<Blocker> for BlockerHandler {
        fn handle(
            &self,
            _cx: JobCtx,
            _job: Blocker,
        ) -> impl Future<Output = anyhow::Result<()>> + Send {
            let handler = self.clone();
            async move {
                handler.started.notify_one();
                handler.release.notified().await;
                Ok(())
            }
        }
    }

    /// Sleeps longer than its timeout.
    #[derive(Debug, Serialize, Deserialize)]
    struct Sleeper;

    impl Job for Sleeper {
        const KIND: &'static str = "lanetest.sleep";
        const TIMEOUT: Duration = Duration::from_millis(50);
    }

    #[derive(Debug, Clone, Copy)]
    struct SleeperHandler;

    impl JobHandler<Sleeper> for SleeperHandler {
        async fn handle(&self, _cx: JobCtx, _job: Sleeper) -> anyhow::Result<()> {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(())
        }
    }

    #[derive(Debug, Clone, Default)]
    struct LaneTest {
        blocker: BlockerHandler,
    }

    impl Module for LaneTest {
        fn name(&self) -> &'static str {
            "lanetest"
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            cx.job(self.blocker.clone())?;
            cx.job(SleeperHandler)
        }
    }

    async fn enqueue<J: Job>(app: &TestApp, job: &J) {
        let platform = app.app().platform();
        let mut tx = platform.db().write().await.unwrap();
        platform.bus().enqueue(&mut tx, job, Enqueue::default()).await.unwrap();
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn a_blocked_lane_does_not_hold_up_mail() {
        let module = LaneTest::default();
        let blocker = module.blocker.clone();
        let app = TestApp::builder().module(module).build().await;
        let shutdown = tokio_util::sync::CancellationToken::new();
        let worker = app.app().worker().clone();
        let running = tokio::spawn({
            let shutdown = shutdown.clone();
            async move { worker.run(shutdown).await }
        });

        enqueue(&app, &Blocker).await;
        tokio::time::timeout(Duration::from_secs(5), blocker.started.notified())
            .await
            .expect("the lane job starts");
        app.client()
            .post("/api/auth/sign-in", &serde_json::json!({ "email": "a@example.com" }))
            .await
            .expect(http::StatusCode::ACCEPTED);
        let sent = tokio::time::timeout(Duration::from_secs(5), async {
            while app.mail.last_to("a@example.com").is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(sent.is_ok(), "mail went out while the lane job was still blocked");

        blocker.release.notify_one();
        shutdown.cancel();
        running.await.unwrap();
    }

    #[tokio::test]
    async fn a_job_past_its_timeout_fails_and_retries() {
        let app = TestApp::builder().module(LaneTest::default()).build().await;
        enqueue(&app, &Sleeper).await;
        let run = app.drain_jobs().await;
        assert_eq!(run.retried, 1, "the timed-out attempt is retried");
        let (status, last_error): (String, Option<String>) =
            sqlx::query_as("SELECT status, last_error FROM jobs WHERE kind = 'lanetest.sleep'")
                .fetch_one(app.app().platform().db().read())
                .await
                .unwrap();
        assert_eq!(status, "queued", "requeued");
        assert!(last_error.unwrap().contains("timed out"), "timeout recorded");
    }

    #[tokio::test]
    async fn draining_covers_every_lane() {
        let module = LaneTest::default();
        let blocker = module.blocker.clone();
        let app = TestApp::builder().module(module).build().await;
        blocker.release.notify_one();
        enqueue(&app, &Blocker).await;
        let mut tx = app.app().platform().db().write().await.unwrap();
        sqlx::query(
            "INSERT INTO jobs (kind, payload, status, attempts, max_attempts, run_at, created_at, updated_at)
             VALUES ('gone.kind', '{}', 'queued', 0, 5, 0, 0, 0)",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let run = app.drain_jobs().await;
        assert_eq!(run.done, 1, "the lane job ran");
        assert_eq!(run.dead, 1, "the unknown kind went dead in the default lane");
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct TooSlow;

    impl Job for TooSlow {
        const KIND: &'static str = "slowtest.job";
        const TIMEOUT: Duration = Duration::from_mins(6);
    }

    #[derive(Debug, Clone, Copy)]
    struct TooSlowHandler;

    impl JobHandler<TooSlow> for TooSlowHandler {
        fn handle(
            &self,
            _cx: JobCtx,
            _job: TooSlow,
        ) -> impl Future<Output = anyhow::Result<()>> + Send {
            std::future::ready(Ok(()))
        }
    }

    #[derive(Debug)]
    struct SlowTest;

    impl Module for SlowTest {
        fn name(&self) -> &'static str {
            "slowtest"
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            cx.job(TooSlowHandler)
        }
    }

    #[tokio::test]
    #[should_panic(expected = "at most")]
    async fn a_timeout_past_the_claim_lock_fails_registration() {
        TestApp::builder().module(SlowTest).build().await;
    }
}
