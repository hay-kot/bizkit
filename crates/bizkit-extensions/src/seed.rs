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

/// Seeds "Post to Discord" (after each submission) and "Block casino spam"
/// (before validation) in the `website` project, bound to the Contact form
/// when the forms seed made one. A script that already exists by name is
/// left alone, except that a project-wide one from an earlier seed gets
/// bound.
pub(crate) async fn seed(db: &Db, seed: &DemoSeed) -> anyhow::Result<()> {
    let Some(&project_id) = seed.project_ids.get("website") else { return Ok(()) };
    let contact = seed.entities.get(&("forms.form".to_owned(), "Contact".to_owned()));
    let existing = store::list_scripts(db.read(), seed.workspace_id, Some(project_id), None)
        .await
        .context("listing the demo scripts")?;
    let scripts = [
        ("Post to Discord", "forms.submission_created", false, POST_TO_DISCORD),
        ("Block casino spam", "forms.pre_validate", true, BLOCK_CASINO),
    ];
    let mut tx = db.write().await.context("starting the seed transaction")?;
    for (name, hook, sync, source) in scripts {
        let slot_taken = |conn| async move {
            Ok::<_, anyhow::Error>(
                sync && store::has_sync_script(
                    conn,
                    seed.workspace_id,
                    project_id,
                    hook,
                    contact.map(String::as_str),
                )
                .await
                .context("checking the sync slot")?,
            )
        };
        if let Some(found) = existing.iter().find(|script| script.name == name) {
            if let (Some(contact), None) = (contact, &found.target_id)
                && !slot_taken(&mut *tx).await?
            {
                store::bind_script(&mut *tx, seed.workspace_id, found.id, contact)
                    .await
                    .with_context(|| format!("binding the {name} script"))?;
            }
            continue;
        }
        if slot_taken(&mut *tx).await? {
            continue;
        }
        let row = ScriptRow {
            id: ScriptId::new(),
            workspace_id: seed.workspace_id,
            project_id,
            hook: hook.to_owned(),
            sync,
            target_id: contact.cloned(),
            name: name.to_owned(),
            source: source.to_owned(),
            enabled: true,
            created_at: seed.now,
            updated_at: seed.now,
        };
        store::insert_script(&mut *tx, &row)
            .await
            .with_context(|| format!("inserting the {name} script"))?;
    }
    tx.commit().await.context("committing the demo scripts")?;
    Ok(())
}
