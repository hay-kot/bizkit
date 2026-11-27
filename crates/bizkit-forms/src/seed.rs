//! Sample forms for the dev stack's demo workspace.

use std::time::Duration;

use bizkit_core::DemoSeed;
use bizkit_core::auth::random_token;
use bizkit_core::db::Db;
use serde_json::json;

use crate::api::{BotCheck, FieldDef, FieldKind, FormId, SubmissionId};
use crate::hooks::FORM;
use crate::ops::DEFAULT_MIN_FILL_MS;
use crate::store::{self, FormRow, SubmissionRow};

fn field(name: &str, label: &str, kind: FieldKind, required: bool) -> FieldDef {
    FieldDef {
        name: name.to_owned(),
        label: Some(label.to_owned()),
        kind,
        required,
        max_length: None,
        options: Vec::new(),
        multiple: false,
    }
}

/// A demo form: name, fields, bot check and sample submissions, each with
/// an optional spam reason.
type DemoForm =
    (&'static str, Vec<FieldDef>, BotCheck, Vec<(serde_json::Value, Option<&'static str>)>);

fn demo_forms() -> [DemoForm; 2] {
    let contact = (
        "Contact",
        vec![
            field("name", "Name", FieldKind::Text, true),
            field("email", "Email", FieldKind::Email, true),
            field("message", "Message", FieldKind::Text, true),
        ],
        BotCheck::Honeypot,
        vec![
            (
                json!({"name": "Ada Lovelace", "email": "ada@example.com", "message": "Do you offer annual billing?"}),
                None,
            ),
            (
                json!({"name": "Grace Hopper", "email": "grace@example.com", "message": "The signup page 404s on mobile."}),
                None,
            ),
            (
                json!({"name": "Alan Turing", "email": "alan@example.com", "message": "Can I get a demo next week?"}),
                None,
            ),
            (
                json!({"name": "Cheap pills", "email": "x@spam.example", "message": "Visit http://spam.example"}),
                Some("honeypot"),
            ),
        ],
    );
    let newsletter = (
        "Newsletter",
        Vec::new(),
        BotCheck::Challenge,
        vec![(json!({"email": "reader@example.com", "source": "footer"}), None)],
    );
    [contact, newsletter]
}

/// Seeds "Contact" and "Newsletter" in the `website` project and records
/// them in `seed.entities`. A form that already exists by name keeps its
/// settings; it gets the sample submissions only while its inbox is empty,
/// which is how forms the dev stack's OpenTofu config made get them.
pub(crate) async fn seed(db: &Db, seed: &mut DemoSeed) -> anyhow::Result<()> {
    let Some(&project_id) = seed.project_ids.get("website") else { return Ok(()) };
    let existing = store::list_forms(db.read(), seed.workspace_id, Some(project_id)).await?;
    let mut tx = db.write().await?;
    for (name, fields, bot_check, samples) in demo_forms() {
        let form_id = match existing.iter().find(|f| f.form.name == name) {
            Some(found) if found.inbox_count > 0 => {
                seed.entities
                    .insert((FORM.id.to_owned(), name.to_owned()), found.form.id.to_string());
                continue;
            }
            // Made by the dev stack's OpenTofu config, still without samples.
            Some(found) => found.form.id,
            None => {
                let form = FormRow {
                    id: FormId::new(),
                    workspace_id: seed.workspace_id,
                    project_id,
                    name: name.to_owned(),
                    enabled: true,
                    fields: serde_json::to_string(&fields)?,
                    bot_check: bot_check.as_str().to_owned(),
                    min_fill_ms: i64::from(DEFAULT_MIN_FILL_MS),
                    redirect_url: None,
                    notify_emails: serde_json::to_string(&[bizkit_core::dev::demo::DEMO_EMAIL])?,
                    challenge_key: random_token(),
                    created_at: seed.now,
                    updated_at: seed.now,
                };
                store::insert_form(&mut *tx, &form).await?;
                form.id
            }
        };
        seed.entities.insert((FORM.id.to_owned(), name.to_owned()), form_id.to_string());
        let count = u64::try_from(samples.len()).unwrap_or(0);
        for (i, (data, spam)) in samples.into_iter().enumerate() {
            let age = Duration::from_hours(count.saturating_sub(u64::try_from(i).unwrap_or(0)));
            store::insert_submission(
                &mut *tx,
                &SubmissionRow {
                    id: SubmissionId::new(),
                    workspace_id: seed.workspace_id,
                    form_id,
                    status: if spam.is_some() { "spam" } else { "inbox" }.to_owned(),
                    spam_reason: spam.map(str::to_owned),
                    data: data.to_string(),
                    user_agent: Some("Mozilla/5.0 (demo seed)".to_owned()),
                    referer: Some("https://www.example.com/contact".to_owned()),
                    created_at: seed.now.minus(age),
                },
            )
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
