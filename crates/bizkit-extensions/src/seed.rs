//! Sample scripts for the dev stack's demo workspace.

use anyhow::Context as _;
use bizkit_core::DemoSeed;
use bizkit_core::db::Db;

use crate::api::ScriptId;
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

/// Seeds "Post to Discord" (after each submission, for every form) and
/// "Block casino spam" (before validation, for the Contact form when the
/// forms seed made one) in the `website` project. A script that already
/// exists by name is left alone, and so is a sync point whose slot another
/// script holds.
pub(crate) async fn seed(db: &Db, seed: &DemoSeed) -> anyhow::Result<()> {
    let Some(&project_id) = seed.project_ids.get("website") else { return Ok(()) };
    let contact = seed.entities.get(&("forms.form".to_owned(), "Contact".to_owned()));
    let existing = store::list_scripts(db.read(), seed.workspace_id, Some(project_id))
        .await
        .context("listing the demo scripts")?;
    let scripts = [
        ("Post to Discord", "forms.submission_created", false, POST_TO_DISCORD, None),
        ("Block casino spam", "forms.pre_validate", true, BLOCK_CASINO, contact),
    ];
    let mut tx = db.write().await.context("starting the seed transaction")?;
    for (name, hook, sync, source, target) in scripts {
        if existing.iter().any(|script| script.name == name) {
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
