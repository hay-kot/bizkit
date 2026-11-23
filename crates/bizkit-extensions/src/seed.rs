//! Sample scripts for the dev stack's demo workspace.

use anyhow::Context as _;
use bizkit_core::DemoSeed;
use bizkit_core::db::Db;

use serde_json::json;

use crate::api::{ScriptId, TestCase};
use crate::store::{self, ScriptRow};

const POST_TO_DISCORD: &str = r"// Posts each submission to a Discord channel. Set the
// DISCORD_WEBHOOK secret to a webhook URL; without it, the script only logs.
export default async function (input, ctx) {
  const { form, submission } = input
  const url = ctx.secrets.DISCORD_WEBHOOK
  if (!url) {
    ctx.log(`No DISCORD_WEBHOOK secret; would post ${form.name} submission ${submission.id}`)
    return
  }
  const lines = Object.entries(submission.data)
    .map(([field, value]) => `**${field}**: ${[].concat(value).join(', ')}`)
  const res = await ctx.fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ content: `New ${form.name} submission\n${lines.join('\n')}` }),
  })
  ctx.log('Discord answered', res.status)
}
";

const BLOCK_CASINO: &str = r"// Marks submissions that mention a casino as spam.
export default function (input) {
  const text = Object.values(input.fields).flat().join(' ').toLowerCase()
  if (text.includes('casino')) return { action: 'spam', reason: 'mentions casino' }
  return { action: 'accept' }
}
";

const LOG_STRIPE: &str = r"// Logs the type of each Stripe event the endpoint captures.
export default function (input, ctx) {
  const type = input.request.json?.type ?? 'unknown'
  ctx.log(`Stripe sent ${type}`)
  return { type }
}
";

fn case(name: &str, input: serde_json::Value, expect: Option<serde_json::Value>) -> TestCase {
    TestCase { name: name.to_owned(), input, expect }
}

fn form() -> serde_json::Value {
    json!({ "id": "0190a5f0-0000-7000-8000-000000000001", "name": "Contact",
            "project_id": "0190a5f0-0000-7000-8000-000000000002" })
}

fn discord_tests() -> Vec<TestCase> {
    let input = json!({
        "form": form(),
        "submission": {
            "id": "0190a5f0-0000-7000-8000-000000000003",
            "created_at": 1_767_225_600_000_i64,
            "data": { "email": "ada@example.com", "message": "Hello there" },
        },
    });
    vec![case("Posts a submission", input, None)]
}

fn stripe_tests() -> Vec<TestCase> {
    let input = json!({
        "endpoint": { "id": "0190a5f0-0000-7000-8000-000000000004", "name": "Stripe",
                      "project_id": "0190a5f0-0000-7000-8000-000000000002" },
        "request": {
            "id": "0190a5f0-0000-7000-8000-000000000005",
            "method": "POST",
            "query": null,
            "headers": { "content-type": "application/json" },
            "content_type": "application/json",
            "body": "{\"type\":\"payment_intent.succeeded\"}",
            "json": { "type": "payment_intent.succeeded" },
            "received_at": "2026-01-01T00:00:00Z",
        },
    });
    vec![case("Logs the event type", input, Some(json!({ "type": "payment_intent.succeeded" })))]
}

fn casino_tests() -> Vec<TestCase> {
    let fields = |message: &str| json!({ "form": form(), "fields": { "email": ["ada@example.com"], "message": [message] } });
    vec![
        case(
            "Casino is spam",
            fields("Win big at our casino"),
            Some(json!({ "action": "spam", "reason": "mentions casino" })),
        ),
        case(
            "A normal message is accepted",
            fields("Hello there"),
            Some(json!({ "action": "accept" })),
        ),
    ]
}

/// Seeds "Post to Discord" (after each submission, for every form),
/// "Block casino spam" (before validation, for the Contact form when the
/// forms seed made one) and "Log Stripe events" (for the Stripe endpoint,
/// only when the webhooks seed made one) in the `website` project. A script
/// that already exists by name is left alone, and so is a sync point whose
/// slot another script holds.
pub(crate) async fn seed(db: &Db, seed: &DemoSeed) -> anyhow::Result<()> {
    let Some(&project_id) = seed.project_ids.get("website") else { return Ok(()) };
    let contact = seed.entities.get(&("forms.form".to_owned(), "Contact".to_owned()));
    let stripe = seed.entities.get(&("webhooks.endpoint".to_owned(), "Stripe".to_owned()));
    let existing = store::list_scripts(db.read(), seed.workspace_id, Some(project_id))
        .await
        .context("listing the demo scripts")?;
    let scripts = [
        (
            "Post to Discord",
            "forms.submission_created",
            false,
            POST_TO_DISCORD,
            None,
            discord_tests(),
        ),
        ("Block casino spam", "forms.pre_validate", true, BLOCK_CASINO, contact, casino_tests()),
        (
            "Log Stripe events",
            "webhooks.request_received",
            false,
            LOG_STRIPE,
            stripe,
            stripe_tests(),
        ),
    ];
    let mut tx = db.write().await.context("starting the seed transaction")?;
    for (name, hook, sync, source, target, tests) in scripts {
        // Without the endpoint, the webhooks module isn't compiled in.
        let missing_endpoint = hook.starts_with("webhooks.") && target.is_none();
        if missing_endpoint || existing.iter().any(|script| script.name == name) {
            continue;
        }
        let row = ScriptRow {
            id: ScriptId::new(),
            workspace_id: seed.workspace_id,
            project_id,
            hook: hook.to_owned(),
            sync,
            target_ids: target.into_iter().cloned().collect(),
            name: name.to_owned(),
            source: source.to_owned(),
            tests,
            enabled: true,
            created_at: seed.now,
            updated_at: seed.now,
        };
        if store::sync_conflict(&mut *tx, &row).await.context("checking the sync slot")?.is_some() {
            continue;
        }
        store::insert_script(&mut tx, &row)
            .await
            .with_context(|| format!("inserting the {name} script"))?;
    }
    tx.commit().await.context("committing the demo scripts")?;
    Ok(())
}
