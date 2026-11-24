//! The analytics hook point: `analytics.goal_converted` runs when a person
//! converts on a goal, at most once per goal, person and UTC day. Scripts
//! bind to goals.

use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use bizkit_core::Platform;
use bizkit_core::hooks::{
    HookKind, HookPoint, HookSource, HookTarget, TargetEntity, TargetKind, TargetResolver,
};
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::jobs::{Event, JobCtx};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::GoalId;
use crate::files::Files;
use crate::report::Scope;
use crate::store::{self, GoalRow, HookEventRow};

/// Published in the ingest transaction when a person converts on a goal for
/// the first time in a UTC day.
#[expect(clippy::struct_field_names, reason = "the IDs are named for what they identify")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalConverted {
    /// The workspace.
    pub workspace_id: WorkspaceId,
    /// The project.
    pub project_id: ProjectId,
    /// The goal.
    pub goal_id: GoalId,
    /// The `analytics_events` row that converted.
    pub event_id: i64,
}

impl Event for GoalConverted {
    const NAME: &'static str = "analytics.goal.converted";
}

/// Scripts on the analytics point can bind to one goal.
pub static GOAL: TargetKind = TargetKind { id: "analytics.goal", noun: "goal", plural: "goals" };

/// Runs as a job when a person converts on a goal.
pub const GOAL_CONVERTED: HookPoint = HookPoint {
    id: "analytics.goal_converted",
    summary: "When a person reaches a goal, at most once per person and day, such as to add a new signup to a mailing list.",
    sample: goal_converted_sample,
    kind: HookKind::Async { event: GoalConverted::NAME },
    target: Some(&GOAL),
};

/// The `analytics.goal_converted` input. `person` is null unless the event
/// belongs to an identified person. No IP, user agent or visitor hash.
pub(crate) fn goal_converted_input(goal: &GoalRow, event: &HookEventRow) -> Value {
    let props = event
        .props
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or_else(|| json!({}));
    let person = event.user_id.as_ref().map(|user_id| {
        let traits = event
            .traits
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .unwrap_or_else(|| json!({}));
        json!({ "user_id": user_id, "traits": traits })
    });
    json!({
        "goal": { "id": goal.id, "name": goal.name },
        "event": { "name": event.name, "path": event.path, "props": props, "ts": event.ts },
        "person": person,
    })
}

fn goal_converted_sample() -> Value {
    json!({
        "goal": { "id": "0190a5f0-0000-7000-8000-000000000001", "name": "Signup" },
        "event": {
            "name": "signup_completed",
            "path": "/signup",
            "props": { "plan": "pro" },
            "ts": "2026-01-01T00:00:00Z",
        },
        "person": { "user_id": "user_123", "traits": { "email": "ada@example.com" } },
    })
}

fn entity(goal: GoalRow) -> TargetEntity {
    TargetEntity { id: goal.id.to_string(), project_id: goal.project_id, label: goal.name }
}

/// Resolves [`GOAL`] targets: goal IDs, labelled by name.
#[derive(Debug, Clone, Copy, Default)]
pub struct GoalTargets;

#[async_trait]
impl TargetResolver for GoalTargets {
    async fn find(
        &self,
        platform: &Platform,
        workspace_id: WorkspaceId,
        ids: &[String],
    ) -> anyhow::Result<Vec<TargetEntity>> {
        let ids: Vec<GoalId> = ids.iter().filter_map(|id| id.parse().ok()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let goals = store::find_goals(platform.db().read(), workspace_id, &ids)
            .await
            .context("loading goals")?;
        Ok(goals.into_iter().map(entity).collect())
    }

    async fn list(
        &self,
        platform: &Platform,
        workspace_id: WorkspaceId,
        project_id: ProjectId,
    ) -> anyhow::Result<Vec<TargetEntity>> {
        let goals = store::list_goals(platform.db().read(), workspace_id, project_id)
            .await
            .context("listing goals")?;
        Ok(goals.into_iter().map(entity).collect())
    }
}

/// Builds the `analytics.goal_converted` target and input from the event.
#[derive(Debug, Clone)]
pub(crate) struct GoalHookSource(pub(crate) Arc<Files>);

impl GoalHookSource {
    /// The goal and event, or None when either is gone (a deleted goal, or
    /// an event pruned before the run).
    async fn load(
        &self,
        cx: &JobCtx,
        event: &GoalConverted,
    ) -> anyhow::Result<Option<(GoalRow, HookEventRow)>> {
        let goal = store::find_goals(cx.platform.db().read(), event.workspace_id, &[event.goal_id])
            .await
            .context("loading the goal")?
            .into_iter()
            .next();
        let Some(goal) = goal else { return Ok(None) };
        let scope = Scope { workspace_id: event.workspace_id, project_id: event.project_id };
        let Some(file) = self.0.get(&cx.platform, scope).await? else { return Ok(None) };
        let row = store::find_hook_event(
            file.db.read(),
            event.workspace_id,
            event.project_id,
            event.event_id,
        )
        .await
        .context("loading the event")?;
        Ok(row.map(|row| (goal, row)))
    }
}

impl HookSource<GoalConverted> for GoalHookSource {
    async fn target(
        &self,
        cx: &JobCtx,
        event: &GoalConverted,
    ) -> anyhow::Result<Option<HookTarget>> {
        Ok(self.load(cx, event).await?.map(|(goal, _)| HookTarget {
            workspace_id: goal.workspace_id,
            project_id: goal.project_id,
            entity: Some(goal.id.to_string()),
        }))
    }

    async fn input(&self, cx: &JobCtx, event: &GoalConverted) -> anyhow::Result<Option<Value>> {
        Ok(self.load(cx, event).await?.map(|(goal, row)| goal_converted_input(&goal, &row)))
    }
}

#[cfg(test)]
mod tests {
    use bizkit_core::time::UnixMillis;

    use super::*;

    fn goal() -> GoalRow {
        GoalRow {
            id: GoalId::new(),
            workspace_id: WorkspaceId::new(),
            project_id: ProjectId::new(),
            name: "Signup".to_owned(),
            kind: "event".to_owned(),
            matches: "signup_completed".to_owned(),
            created_at: UnixMillis(0),
        }
    }

    fn event(user_id: Option<&str>) -> HookEventRow {
        HookEventRow {
            name: "signup_completed".to_owned(),
            path: Some("/signup".to_owned()),
            props: Some(r#"{"plan":"pro"}"#.to_owned()),
            ts: UnixMillis(0),
            user_id: user_id.map(str::to_owned),
            traits: user_id.map(|_| r#"{"email":"ada@x.test"}"#.to_owned()),
        }
    }

    #[test]
    fn identified_events_carry_the_person() {
        let input = goal_converted_input(&goal(), &event(Some("user_1")));
        assert_eq!(input["goal"]["name"], "Signup");
        assert_eq!(input["event"]["props"], json!({"plan": "pro"}));
        assert_eq!(
            input["person"],
            json!({"user_id": "user_1", "traits": {"email": "ada@x.test"}})
        );
    }

    #[test]
    fn anonymous_events_have_no_person() {
        let input = goal_converted_input(&goal(), &event(None));
        assert_eq!(input["person"], Value::Null);
    }

    #[test]
    fn the_sample_matches_the_input_shape() {
        let input = goal_converted_input(&goal(), &event(Some("u")));
        let sample = goal_converted_sample();
        for key in ["goal", "event", "person"] {
            assert!(sample.get(key).is_some() && input.get(key).is_some(), "{key}");
        }
    }
}
