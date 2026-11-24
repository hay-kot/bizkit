//! The mailing hook point: `mailing.step_condition` runs before each
//! sequence step and can skip it.

use std::time::Duration;

use anyhow::Context as _;
use async_trait::async_trait;
use bizkit_core::Platform;
use bizkit_core::hooks::{HookKind, HookPoint, TargetEntity, TargetKind, TargetResolver};
use bizkit_core::ids::{ProjectId, WorkspaceId};
use serde_json::{Value, json};

use crate::api::SequenceId;
use crate::store::{self, ContactRow, SequenceRow, StepRow};

/// The longest skip reason a script can set.
pub const MAX_REASON: usize = 200;

/// Step-condition scripts bind to one sequence.
pub static SEQUENCE: TargetKind =
    TargetKind { id: "mailing.sequence", noun: "sequence", plural: "sequences" };

/// Runs before each sequence step is sent.
pub const STEP_CONDITION: HookPoint = HookPoint {
    id: "mailing.step_condition",
    summary: "Before each sequence step is sent. Return nothing to send it, or {action: 'skip', reason}.",
    sample: step_condition_sample,
    kind: HookKind::Sync { budget: Duration::from_millis(50), output: check_condition },
    target: Some(&SEQUENCE),
};

/// The `mailing.step_condition` input.
pub(crate) fn step_condition_input(
    sequence: &SequenceRow,
    step: &StepRow,
    contact: &ContactRow,
) -> anyhow::Result<Value> {
    let tags: Value = serde_json::from_str(&contact.tags)
        .with_context(|| format!("decoding the tags of contact {}", contact.id))?;
    Ok(json!({
        "sequence": { "id": sequence.id, "name": sequence.name },
        "step": { "id": step.id, "position": step.position, "subject": step.subject },
        "contact": {
            "email": contact.email,
            "name": contact.name,
            "tags": tags,
            "subscribed_at": contact.subscribed_at,
        },
    }))
}

fn step_condition_sample() -> Value {
    json!({
        "sequence": { "id": "0190a5f0-0000-7000-8000-000000000001", "name": "Welcome" },
        "step": {
            "id": "0190a5f0-0000-7000-8000-000000000002",
            "position": 1,
            "subject": "Getting started",
        },
        "contact": {
            "email": "ada@example.com",
            "name": "Ada",
            "tags": ["trial"],
            "subscribed_at": "2026-01-01T00:00:00Z",
        },
    })
}

/// What a step-condition script decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Condition {
    Send,
    Skip(String),
}

/// Decodes a step-condition script's return value.
pub(crate) fn condition(value: &Value) -> Result<Condition, String> {
    let Some(object) = value.as_object() else {
        return if value.is_null() {
            Ok(Condition::Send)
        } else {
            Err("return nothing or an object with an action".to_owned())
        };
    };
    match object.get("action").and_then(Value::as_str) {
        Some("send") => Ok(Condition::Send),
        Some("skip") => {
            let reason = object.get("reason").and_then(Value::as_str).map_or("", str::trim);
            let reason = if reason.is_empty() { "script" } else { reason };
            Ok(Condition::Skip(reason.chars().take(MAX_REASON).collect()))
        }
        _ => Err("action must be send or skip".to_owned()),
    }
}

fn check_condition(value: &Value) -> Result<(), String> {
    condition(value).map(|_| ())
}

/// Resolves [`SEQUENCE`] targets: sequence IDs, labelled by name.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SequenceTargets;

#[async_trait]
impl TargetResolver for SequenceTargets {
    async fn find(
        &self,
        platform: &Platform,
        workspace_id: WorkspaceId,
        ids: &[String],
    ) -> anyhow::Result<Vec<TargetEntity>> {
        let ids: Vec<SequenceId> = ids.iter().filter_map(|id| id.parse().ok()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = store::find_sequences(platform.db().read(), workspace_id, &ids)
            .await
            .context("loading sequences")?;
        Ok(rows
            .into_iter()
            .map(|(row, project_id)| TargetEntity {
                id: row.id.to_string(),
                project_id,
                label: row.name,
            })
            .collect())
    }

    async fn list(
        &self,
        platform: &Platform,
        workspace_id: WorkspaceId,
        project_id: ProjectId,
    ) -> anyhow::Result<Vec<TargetEntity>> {
        let rows =
            store::list_sequences(platform.db().read(), workspace_id, Some(project_id), None)
                .await
                .context("listing sequences")?;
        Ok(rows
            .into_iter()
            .map(|row| TargetEntity { id: row.id.to_string(), project_id, label: row.name })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions_decode_send_skip_and_refuse_other_shapes() {
        assert_eq!(condition(&Value::Null), Ok(Condition::Send));
        assert_eq!(condition(&json!({"action": "send"})), Ok(Condition::Send));
        assert_eq!(
            condition(&json!({"action": "skip", "reason": " not a trial "})),
            Ok(Condition::Skip("not a trial".to_owned()))
        );
        assert_eq!(condition(&json!({"action": "skip"})), Ok(Condition::Skip("script".to_owned())));
        let long = "x".repeat(MAX_REASON + 10);
        let Ok(Condition::Skip(reason)) = condition(&json!({"action": "skip", "reason": long}))
        else {
            panic!("skip expected");
        };
        assert_eq!(reason.chars().count(), MAX_REASON, "capped");
        assert!(condition(&json!({"action": "stop"})).is_err());
        assert!(condition(&json!(true)).is_err());
    }

    #[test]
    fn the_sample_matches_the_input_shape() {
        let sample = step_condition_sample();
        for key in ["sequence", "step", "contact"] {
            assert!(sample.get(key).is_some(), "{key}");
        }
    }
}
