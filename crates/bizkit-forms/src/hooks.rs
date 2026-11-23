//! The forms hook points: `forms.pre_validate` runs in the submit request
//! before field validation, and `forms.submission_created` runs after each
//! inbox submission.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Context as _;
use async_trait::async_trait;
use bizkit_core::Platform;
use bizkit_core::hooks::{
    HookKind, HookPoint, HookSource, HookTarget, TargetEntity, TargetKind, TargetResolver,
};
use bizkit_core::ids::WorkspaceId;
use bizkit_core::jobs::{Event, JobCtx};
use bizkit_core::validate::FieldErrors;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::{FormId, SubmissionId};
use crate::fields;
use crate::ingest::parse::RawFields;
use crate::store::{self, FormRow};

/// The longest spam reason or error message a script can set.
pub const MAX_SCRIPT_TEXT: usize = 200;
/// The most fields a script's reject verdict can name.
const MAX_SCRIPT_ERRORS: usize = 50;

/// Published in the insert transaction for each inbox submission.
#[expect(clippy::struct_field_names, reason = "the IDs are named for what they identify")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionCreated {
    /// The workspace.
    pub workspace_id: WorkspaceId,
    /// The form.
    pub form_id: FormId,
    /// The new submission.
    pub submission_id: SubmissionId,
}

impl Event for SubmissionCreated {
    const NAME: &'static str = "forms.submission.created";
}

/// Scripts on the forms points can bind to one form.
pub static FORM: TargetKind = TargetKind { id: "forms.form", noun: "form", plural: "forms" };

/// Runs in the submit request after the bot checks pass, before field
/// validation.
pub const PRE_VALIDATE: HookPoint = HookPoint {
    id: "forms.pre_validate",
    summary: "Before field validation, for submissions that passed the bot checks. Return nothing or {action: 'accept'}, {action: 'reject', errors: {field: ['message']}}, or {action: 'spam', reason}.",
    sample: pre_validate_sample,
    kind: HookKind::Sync { budget: Duration::from_millis(50), output: check_verdict },
    target: Some(&FORM),
};

/// Runs as a job after each inbox submission is stored.
pub const SUBMISSION_CREATED: HookPoint = HookPoint {
    id: "forms.submission_created",
    summary: "After each inbox submission is stored, such as to post it to a chat channel.",
    sample: submission_created_sample,
    kind: HookKind::Async { event: SubmissionCreated::NAME },
    target: Some(&FORM),
};

fn form_json(form: &FormRow) -> Value {
    json!({ "id": form.id, "name": form.name, "project_id": form.project_id })
}

/// The `forms.pre_validate` input. Control fields are left out.
pub(crate) fn pre_validate_input(form: &FormRow, raw: &RawFields) -> Value {
    let fields: BTreeMap<&str, &Vec<String>> = raw
        .0
        .iter()
        .filter(|(name, _)| !fields::is_control(name))
        .map(|(name, values)| (name.as_str(), values))
        .collect();
    json!({ "form": form_json(form), "fields": fields })
}

fn pre_validate_sample() -> Value {
    json!({
        "form": {
            "id": "0190a5f0-0000-7000-8000-000000000001",
            "name": "Contact",
            "project_id": "0190a5f0-0000-7000-8000-000000000002",
        },
        "fields": { "email": ["ada@example.com"], "message": ["Hello there"] },
    })
}

fn submission_created_sample() -> Value {
    json!({
        "form": {
            "id": "0190a5f0-0000-7000-8000-000000000001",
            "name": "Contact",
            "project_id": "0190a5f0-0000-7000-8000-000000000002",
        },
        "submission": {
            "id": "0190a5f0-0000-7000-8000-000000000003",
            "created_at": 1_767_225_600_000_i64,
            "data": { "email": "ada@example.com", "message": "Hello there" },
        },
    })
}

/// What a pre-validate script decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Go on to field validation.
    Accept,
    /// Refuse with these field errors (422).
    Reject(FieldErrors),
    /// Store as spam with this reason.
    Spam(String),
}

fn capped(text: &str) -> String {
    text.chars().take(MAX_SCRIPT_TEXT).collect()
}

/// Decodes a pre-validate script's return value.
pub(crate) fn verdict(value: &Value) -> Result<Verdict, String> {
    let Some(object) = value.as_object() else {
        return if value.is_null() {
            Ok(Verdict::Accept)
        } else {
            Err("return nothing or an object with an action".to_owned())
        };
    };
    match object.get("action").and_then(Value::as_str) {
        Some("accept") => Ok(Verdict::Accept),
        Some("spam") => {
            let reason = object.get("reason").and_then(Value::as_str).map_or("", str::trim);
            Ok(Verdict::Spam(if reason.is_empty() { "script".to_owned() } else { capped(reason) }))
        }
        Some("reject") => reject(object.get("errors")).map(Verdict::Reject),
        _ => Err("action must be accept, reject or spam".to_owned()),
    }
}

fn reject(errors: Option<&Value>) -> Result<FieldErrors, String> {
    let shape = || "reject needs errors: {field: ['message']}".to_owned();
    let map = errors.and_then(Value::as_object).ok_or_else(shape)?;
    if map.is_empty() || map.len() > MAX_SCRIPT_ERRORS {
        return Err(format!("reject needs 1 to {MAX_SCRIPT_ERRORS} fields in errors"));
    }
    let mut out = FieldErrors::default();
    for (field, messages) in map {
        let messages = messages.as_array().ok_or_else(shape)?;
        if messages.is_empty() {
            return Err(shape());
        }
        for message in messages {
            out.add(capped(field), capped(message.as_str().ok_or_else(shape)?));
        }
    }
    Ok(out)
}

fn check_verdict(value: &Value) -> Result<(), String> {
    verdict(value).map(|_| ())
}

/// Resolves [`FORM`] targets: form IDs, labelled by form name.
#[derive(Debug, Clone, Copy, Default)]
pub struct FormTargets;

#[async_trait]
impl TargetResolver for FormTargets {
    async fn find(
        &self,
        platform: &Platform,
        workspace_id: WorkspaceId,
        ids: &[String],
    ) -> anyhow::Result<Vec<TargetEntity>> {
        let ids: Vec<FormId> = ids.iter().filter_map(|id| id.parse().ok()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let forms = store::find_forms(platform.db().read(), workspace_id, &ids)
            .await
            .context("loading forms")?;
        Ok(forms
            .into_iter()
            .map(|form| TargetEntity {
                id: form.id.to_string(),
                project_id: form.project_id,
                label: form.name,
            })
            .collect())
    }
}

/// Builds the `forms.submission_created` target and input from the event.
#[derive(Debug, Clone, Copy, Default)]
pub struct FormsHookSource;

impl HookSource<SubmissionCreated> for FormsHookSource {
    async fn target(
        &self,
        cx: &JobCtx,
        event: &SubmissionCreated,
    ) -> anyhow::Result<Option<HookTarget>> {
        let form = store::find_form(cx.platform.db().read(), event.workspace_id, event.form_id)
            .await
            .context("loading the form")?;
        Ok(form.map(|form| HookTarget {
            workspace_id: form.workspace_id,
            project_id: form.project_id,
            entity: Some(form.id.to_string()),
        }))
    }

    async fn input(&self, cx: &JobCtx, event: &SubmissionCreated) -> anyhow::Result<Option<Value>> {
        let db = cx.platform.db().read();
        let Some(form) = store::find_form(db, event.workspace_id, event.form_id)
            .await
            .context("loading the form")?
        else {
            return Ok(None);
        };
        let Some(submission) =
            store::find_submission(db, event.workspace_id, event.form_id, event.submission_id)
                .await
                .context("loading the submission")?
        else {
            return Ok(None);
        };
        let data: Value =
            serde_json::from_str(&submission.data).context("decoding the submission data")?;
        Ok(Some(json!({
            "form": form_json(&form),
            "submission": { "id": submission.id, "created_at": submission.created_at, "data": data },
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(pairs: &[(&str, &str)]) -> FieldErrors {
        let mut errors = FieldErrors::default();
        for (field, message) in pairs {
            errors.add(*field, *message);
        }
        errors
    }

    #[test]
    fn verdicts_decode() {
        assert_eq!(verdict(&Value::Null), Ok(Verdict::Accept), "nothing accepts");
        assert_eq!(verdict(&json!({"action": "accept"})), Ok(Verdict::Accept), "accept");
        assert_eq!(
            verdict(&json!({"action": "spam", "reason": " casino "})),
            Ok(Verdict::Spam("casino".to_owned())),
            "spam with a reason"
        );
        assert_eq!(
            verdict(&json!({"action": "spam"})),
            Ok(Verdict::Spam("script".to_owned())),
            "spam without a reason"
        );
        assert_eq!(
            verdict(&json!({"action": "reject", "errors": {"email": ["is blocked"]}})),
            Ok(Verdict::Reject(errors(&[("email", "is blocked")]))),
            "reject"
        );
    }

    #[test]
    fn script_text_is_capped() {
        let long = "x".repeat(500);
        let Ok(Verdict::Spam(reason)) = verdict(&json!({"action": "spam", "reason": long})) else {
            panic!("spam");
        };
        assert_eq!(reason.chars().count(), MAX_SCRIPT_TEXT, "reason capped");
    }

    #[test]
    fn bad_shapes_are_refused() {
        for value in [
            json!(true),
            json!("accept"),
            json!({}),
            json!({"action": "maybe"}),
            json!({"action": "reject"}),
            json!({"action": "reject", "errors": {}}),
            json!({"action": "reject", "errors": {"email": "is blocked"}}),
            json!({"action": "reject", "errors": {"email": []}}),
            json!({"action": "reject", "errors": {"email": [1]}}),
        ] {
            assert!(verdict(&value).is_err(), "{value} is refused");
        }
    }

    #[test]
    fn the_input_drops_control_fields() {
        let form = FormRow {
            id: FormId::new(),
            workspace_id: WorkspaceId::new(),
            project_id: bizkit_core::ids::ProjectId::new(),
            name: "Contact".to_owned(),
            enabled: true,
            fields: "[]".to_owned(),
            bot_check: "honeypot".to_owned(),
            min_fill_ms: 0,
            redirect_url: None,
            notify_emails: "[]".to_owned(),
            challenge_key: String::new(),
            created_at: bizkit_core::time::UnixMillis(0),
            updated_at: bizkit_core::time::UnixMillis(0),
        };
        let mut raw = RawFields::default();
        raw.0.insert("email".to_owned(), vec!["a@example.com".to_owned()]);
        raw.0.insert("_gotcha".to_owned(), vec![String::new()]);
        raw.0.insert("altcha".to_owned(), vec!["proof".to_owned()]);
        let input = pre_validate_input(&form, &raw);
        assert_eq!(input["fields"], json!({"email": ["a@example.com"]}), "control fields gone");
        assert_eq!(input["form"]["name"], "Contact", "form");
    }
}
