// Prints OpenTofu import blocks for demo rows that exist before dev/tofu
// manages them (a stack seeded by the Rust seed alone). Names match
// dev/tofu/main.tf. Reads BIZKIT_ENDPOINT, BIZKIT_API_KEY and
// BIZKIT_WORKSPACE_ID.
const { BIZKIT_ENDPOINT: endpoint, BIZKIT_API_KEY: key, BIZKIT_WORKSPACE_ID: ws } = process.env

async function get(path) {
  const res = await fetch(`${endpoint}/api/v1/workspaces/${ws}${path}`, {
    headers: { authorization: `Bearer ${key}` },
  })
  if (!res.ok) throw new Error(`GET ${path}: ${res.status} ${await res.text()}`)
  return (await res.json()).items
}

const blocks = []
const adopt = (to, id) => blocks.push(`import {\n  to = ${to}\n  id = "${id}"\n}\n`)

const projects = await get('/projects?limit=500')
for (const slug of ['website', 'docs', 'bizkit']) {
  const project = projects.find((p) => p.slug === slug)
  if (project) adopt(`bizkit_project.this["${slug}"]`, project.id)
}

const website = projects.find((p) => p.slug === 'website')
if (website) {
  const q = `?project_id=${website.id}`
  const byName = (items, name) => items.find((item) => item.name === name)
  const forms = await get(`/forms${q}`)
  const endpoints = await get(`/webhooks${q}`)
  const lists = await get(`/mailing/lists${q}`)
  for (const [to, row] of [
    ['bizkit_form.contact', byName(forms, 'Contact')],
    ['bizkit_form.newsletter', byName(forms, 'Newsletter')],
    ['bizkit_webhook_endpoint.stripe', byName(endpoints, 'Stripe')],
    ['bizkit_webhook_endpoint.github', byName(endpoints, 'GitHub')],
    ['bizkit_mailing_list.newsletter', byName(lists, 'Newsletter')],
    ['bizkit_mailing_list.updates', byName(lists, 'Product updates')],
  ]) {
    if (row) adopt(to, row.id)
  }
}

process.stdout.write(blocks.join('\n'))
