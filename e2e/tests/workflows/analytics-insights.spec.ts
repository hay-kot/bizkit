import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('build a per-user insight, save it and pin it to a new dashboard', async ({
  page,
  playwright,
  world,
  baseURL,
}) => {
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const api = await playwright.request.newContext({
    baseURL,
    extraHTTPHeaders: { authorization: `Bearer ${world.api_keys.full!.token}` },
  })
  try {
    const base = `/api/v1/workspaces/${workspace}/analytics`
    expect((await api.post(`${base}/site`, { data: { project_id: project } })).status()).toBe(201)
    const events = [
      ...Array.from({ length: 3 }, () => ({ name: 'workspace_created', user_id: 'ada' })),
      { name: 'workspace_created', user_id: 'bob' },
    ]
    expect((await api.post(`${base}/events`, { data: { project_id: project, events } })).status()).toBe(200)
    // Events land after the writer's flush window; open the page once they have.
    const hasEvents = async () => {
      const site = (await (await api.get(`${base}/site?project_id=${project}`)).json()) as { has_events: boolean }
      return site.has_events
    }
    await expect.poll(hasEvents).toBe(true)
  } finally {
    await api.dispose()
  }

  await page.goto(`/w/${workspace}/p/${project}/analytics/insights`)
  await page.getByRole('link', { name: 'New insight' }).first().click()
  await page.getByRole('combobox', { name: 'Event' }).click()
  await page.getByRole('option', { name: 'workspace_created' }).click()
  await page.getByRole('combobox', { name: 'Measure' }).click()
  await page.getByRole('option', { name: /Per-user distribution/ }).click()
  const bars = page.getByRole('listitem').filter({ hasText: /times?/ })
  await expect(bars).toHaveText(
    ['1 time Users: 1', '2 times Users: 0', '3 times Users: 1', '4 times Users: 0', '5+ times Users: 0'],
    {
      useInnerText: true,
    },
  )

  await page.getByRole('button', { name: 'Save insight' }).click()
  const save = page.getByRole('dialog', { name: 'Save insight' })
  await save.getByLabel('Name').fill('Workspaces per user')
  await save.getByRole('button', { name: 'Save insight' }).click()
  await expect(page.getByRole('heading', { name: 'Workspaces per user' })).toBeVisible()

  await page.getByRole('button', { name: 'Add to dashboard' }).click()
  const pin = page.getByRole('dialog', { name: 'Add to dashboard' })
  await pin.getByLabel('New dashboard name').fill('Adoption')
  await pin.getByRole('button', { name: 'Add to dashboard' }).click()
  await expect(page.getByRole('heading', { name: 'Adoption' })).toBeVisible()
  const tile = page.getByRole('region', { name: 'Workspaces per user' })
  await expect(tile).toContainText('2')
  await expect(tile.getByRole('listitem')).toHaveCount(5)
})
