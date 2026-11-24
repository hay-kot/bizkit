import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('an attribute set on a contact picks the recipients of a broadcast', async ({
  page,
  world,
  baseURL,
  namespace,
}) => {
  const workspace = world.workspaces.main!.id
  const headers = { Origin: new URL(baseURL!).origin }
  const ann = `ann+${namespace}@e2e.test`
  const bob = `bob+${namespace}@e2e.test`
  const created = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/lists`, {
    data: { name: 'Newsletter', project_id: world.projects.site!.id, double_opt_in: false },
    headers,
  })
  expect(created.status()).toBe(201)
  const list = await created.json()
  const contacts = `/api/v1/workspaces/${workspace}/mailing/lists/${list.id}/contacts`
  for (const email of [ann, bob]) {
    expect((await page.request.post(contacts, { data: { email }, headers })).status()).toBe(201)
  }
  const drafted = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/broadcasts`, {
    data: { list_id: list.id, subject: `Pro ${namespace}`, body_md: 'You are on {{ attr.plan }}.' },
    headers,
  })
  const broadcast = await drafted.json()
  const project = `/w/${workspace}/p/${world.projects.site!.id}`

  await page.goto(`${project}/mailing/lists/${list.id}`)
  const annRow = page.getByRole('row', { name: new RegExp(ann.replace(/[+.]/g, '\\$&')) })
  await annRow.getByText(ann).click()
  await page.getByRole('button', { name: 'Add attribute' }).click()
  await page.getByLabel('Key 1').fill('plan')
  await page.getByLabel('Value 1').fill('pro')
  await page.getByRole('button', { name: 'Save contact' }).click()
  await expect(annRow).toContainText('plan: pro')

  await page.goto(`${project}/mailing/broadcasts/${broadcast.id}`)
  await expect(page.getByRole('status').filter({ hasText: 'Matches 2 subscribers.' })).toBeVisible()
  await page.getByRole('button', { name: 'Add condition' }).click()
  await page.getByLabel('Key 1').fill('plan')
  await page.getByLabel('Value 1').fill('pro')
  await expect(page.getByRole('status').filter({ hasText: 'Matches 1 subscriber.' })).toBeVisible()
  await page.getByRole('button', { name: 'Save' }).click()
  await expect(page.getByRole('button', { name: 'Saved' })).toBeVisible()
  await expect(page.getByText('plan is pro')).toBeVisible()

  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await page.getByRole('button', { name: 'Send now' }).click()
  const log = page.getByRole('table', { name: 'Send log' })
  await expect(log.getByRole('row', { name: new RegExp(ann.replace(/[+.]/g, '\\$&')) })).toBeVisible({
    timeout: 15_000,
  })
  await expect(log.getByRole('row', { name: new RegExp(bob.replace(/[+.]/g, '\\$&')) })).toHaveCount(0)
})
