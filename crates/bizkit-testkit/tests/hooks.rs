//! Hook points: sync calls fail open, async points fan events out to the runner.

#[cfg(test)]
mod hooks {
    use std::time::{Duration, Instant};

    use bizkit_core::hooks::{
        HookKind, HookPoint, HookResult, HookSource, HookTarget, RunStatus, SyncOutcome,
    };
    use bizkit_core::ids::{ProjectId, WorkspaceId};
    use bizkit_core::jobs::{Event, JobCtx};
    use bizkit_core::{Module, ModuleContext};
    use bizkit_testkit::{FakeHooks, SyncCall, TestApp};
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, Serialize, Deserialize)]
    struct Happened {
        workspace_id: WorkspaceId,
        project_id: ProjectId,
        skip: bool,
    }

    impl Event for Happened {
        const NAME: &'static str = "hooktest.happened";
    }

    struct Source;

    impl HookSource<Happened> for Source {
        fn target(
            &self,
            _cx: &JobCtx,
            event: &Happened,
        ) -> impl Future<Output = anyhow::Result<Option<HookTarget>>> + Send {
            std::future::ready(Ok((!event.skip).then_some(HookTarget {
                workspace_id: event.workspace_id,
                project_id: event.project_id,
            })))
        }

        fn input(
            &self,
            _cx: &JobCtx,
            event: &Happened,
        ) -> impl Future<Output = anyhow::Result<Option<serde_json::Value>>> + Send {
            std::future::ready(Ok(Some(json!({ "project": event.project_id.to_string() }))))
        }
    }

    fn sample() -> serde_json::Value {
        json!({ "value": 1 })
    }

    fn needs_ok(value: &serde_json::Value) -> Result<(), String> {
        if value.get("ok").is_some() { Ok(()) } else { Err("missing ok".to_owned()) }
    }

    const CHECK: HookPoint = HookPoint {
        id: "hooktest.check",
        summary: "Checks a thing",
        sample,
        kind: HookKind::Sync { budget: Duration::from_millis(20), output: needs_ok },
    };

    const AFTER: HookPoint = HookPoint {
        id: "hooktest.after",
        summary: "After a thing",
        sample,
        kind: HookKind::Async { event: Happened::NAME },
    };

    #[derive(Debug)]
    struct HookTest;

    impl Module for HookTest {
        fn name(&self) -> &'static str {
            "hooktest"
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            cx.sync_hook(CHECK)?;
            cx.async_hook::<Happened, _>(AFTER, Source)
        }
    }

    fn target() -> HookTarget {
        HookTarget { workspace_id: WorkspaceId::new(), project_id: ProjectId::new() }
    }

    async fn app(hooks: &FakeHooks) -> TestApp {
        TestApp::builder().module(HookTest).hook_runner(hooks.clone()).build().await
    }

    #[tokio::test]
    async fn without_a_runner_points_are_inert() {
        let app = TestApp::builder().module(HookTest).build().await;
        let platform = app.app().platform();
        let ids: Vec<&str> = platform.hook_points().iter().map(|point| point.id).collect();
        assert_eq!(ids, ["hooktest.check", "hooktest.after"], "points are listed");
        let result = platform.call_sync_hook(&CHECK, target(), &json!({})).await;
        assert_eq!(result, HookResult::Continue, "no runner, no script");
    }

    #[tokio::test]
    async fn a_returned_value_reaches_the_caller() {
        let hooks = FakeHooks::default();
        hooks.answer(CHECK.id, SyncOutcome::Returned(json!({ "ok": true })));
        let app = app(&hooks).await;
        let target = target();
        let result = app.app().platform().call_sync_hook(&CHECK, target, &json!({ "a": 1 })).await;
        assert_eq!(result, HookResult::Returned(json!({ "ok": true })), "returned");
        assert_eq!(
            hooks.calls(),
            [SyncCall { point: CHECK.id, target, input: json!({ "a": 1 }) }],
            "the runner saw the call"
        );
    }

    #[tokio::test]
    async fn sync_hooks_fail_open() {
        let hooks = FakeHooks::default();
        let app = app(&hooks).await;
        let platform = app.app().platform();
        for status in
            [RunStatus::Error, RunStatus::Timeout, RunStatus::Overloaded, RunStatus::InvalidOutput]
        {
            hooks.answer(CHECK.id, SyncOutcome::Failed { status, message: "boom".to_owned() });
            let result = platform.call_sync_hook(&CHECK, target(), &json!({})).await;
            assert_eq!(result, HookResult::Continue, "{status} continues");
        }
        hooks.answer(CHECK.id, SyncOutcome::Returned(json!({ "nope": 1 })));
        let result = platform.call_sync_hook(&CHECK, target(), &json!({})).await;
        assert_eq!(result, HookResult::Continue, "core checks the output shape too");
    }

    #[tokio::test]
    async fn a_stuck_runner_cant_hold_the_request() {
        let hooks = FakeHooks::default();
        hooks.hang(CHECK.id);
        let app = app(&hooks).await;
        let start = Instant::now();
        let result = app.app().platform().call_sync_hook(&CHECK, target(), &json!({})).await;
        assert_eq!(result, HookResult::Continue, "continues");
        assert!(start.elapsed() < Duration::from_secs(3), "gave up after the grace period");
    }

    #[tokio::test]
    async fn sync_calls_on_async_points_continue() {
        let hooks = FakeHooks::default();
        let app = app(&hooks).await;
        let result = app.app().platform().call_sync_hook(&AFTER, target(), &json!({})).await;
        assert_eq!(result, HookResult::Continue, "continues");
        assert!(hooks.calls().is_empty(), "the runner was never asked");
    }

    async fn publish(app: &TestApp, event: &Happened) {
        let platform = app.app().platform();
        let mut tx = platform.db().write().await.unwrap();
        platform.bus().publish(&mut tx, event).await.unwrap();
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn events_fan_out_to_the_runner() {
        let hooks = FakeHooks::default();
        let app = app(&hooks).await;
        let target = target();
        let event = Happened {
            workspace_id: target.workspace_id,
            project_id: target.project_id,
            skip: false,
        };
        publish(&app, &event).await;
        publish(&app, &Happened { skip: true, ..event }).await;
        assert_eq!(app.drain_jobs().await.done, 2, "both deliveries ran");

        let dispatches = hooks.dispatches();
        assert_eq!(dispatches.len(), 1, "the skipped event never reached the runner");
        assert_eq!((dispatches[0].point, dispatches[0].target), (AFTER.id, target), "routed");

        let platform = app.app().platform();
        let cx = JobCtx { platform: platform.clone(), job_id: 0, attempt: 1, request_id: None };
        let input = platform.hook_input(&cx, AFTER.id, &dispatches[0].event).await.unwrap();
        assert_eq!(input, Some(json!({ "project": target.project_id.to_string() })), "rebuilt");
        let unknown = platform.hook_input(&cx, "gone.after", &dispatches[0].event).await.unwrap();
        assert_eq!(unknown, None, "a point no module declares skips");
    }
}
