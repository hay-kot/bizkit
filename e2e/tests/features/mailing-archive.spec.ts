import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('a sent broadcast marked for the archive shows on the public page and feed', async ({
  page,
  world,
  baseURL,
  namespace,
}) => {
  const workspace = world.workspaces.main!.id
  const headers = { Origin: new URL(baseURL!).origin }
  const created = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/lists`, {
    data: { name: `Archive ${namespace}`, project_id: world.projects.site!.id, double_opt_in: false },
    headers,
  })
  expect(created.status()).toBe(201)
  const list = await created.json()
  const contacts = `/api/v1/workspaces/${workspace}/mailing/lists/${list.id}/contacts`
  const reader = { email: `ann+${namespace}@e2e.test`, name: 'Ann' }
  expect((await page.request.post(contacts, { data: reader, headers })).status()).toBe(201)
  const subject = `Issue ${namespace}`
  const drafted = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/broadcasts`, {
    data: { list_id: list.id, subject, body_md: 'Hello {{ name }}, here is the news.' },
    headers,
  })
  const broadcast = await drafted.json()
  const send = `/api/v1/workspaces/${workspace}/mailing/broadcasts/${broadcast.id}/send`
  expect((await page.request.post(send, { data: {}, headers })).status()).toBe(200)
  const project = `/w/${workspace}/p/${world.projects.site!.id}`

  await page.goto(`${project}/mailing/lists/${list.id}`)
  await page.getByRole('button', { name: 'Settings' }).click()
  await page.getByLabel('Publish a public archive').check()
  await page.getByRole('button', { name: 'Save settings' }).click()
  await expect(page.getByRole('link', { name: 'Open' })).toBeVisible()

  await page.goto(`${project}/mailing/broadcasts/${broadcast.id}`)
  await page.getByLabel('Show in the list’s public archive').check()
  await expect(page.getByRole('link', { name: 'Open in the archive' })).toBeVisible()

  const archive = new URL(list.archive_url).pathname
  await page.goto(archive)
  await page.getByRole('link', { name: subject }).click()
  await expect(page.getByRole('heading', { name: subject })).toBeVisible()
  await expect(page.getByText('Hello , here is the news.')).toBeVisible()

  const feed = await page.request.get(`${archive}/feed.xml`)
  expect(feed.headers()['content-type']).toContain('application/rss+xml')
  expect(await feed.text()).toContain(`<title>${subject}</title>`)
})
