import type { APIRequestContext } from '@playwright/test'

import { expectCleanPage, trackPageErrors } from '../../fixtures/clean-page'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

interface Tile {
  insight_id: string
  width: number
}

/**
 * Gives the project a site with one event, so the analytics pages open,
 * saves the named insights and a dashboard with the first `onBoard` of them
 * at half width. Returns the insight IDs by name and the dashboard ID.
 */
async function setup(api: APIRequestContext, base: string, project: string, names: string[], onBoard: number) {
  expect((await api.post(`${base}/site`, { data: { project_id: project } })).status()).toBe(201)
  const events = [{ name: 'signed_in', user_id: 'ada' }]
  expect((await api.post(`${base}/events`, { data: { project_id: project, events } })).status()).toBe(200)
  const hasEvents = async () => {
    const site = (await (await api.get(`${base}/site?project_id=${project}`)).json()) as { has_events: boolean }
    return site.has_events
  }
  await expect.poll(hasEvents).toBe(true)
  const ids: Record<string, string> = {}
  for (const name of names) {
    const res = await api.post(`${base}/insights`, {
      data: { project_id: project, name, query: { kind: 'events', event: 'signed_in', measure: 'users' } },
    })
    expect(res.status()).toBe(201)
    ids[name] = ((await res.json()) as { id: string }).id
  }
  const tiles = names.slice(0, onBoard).map((name) => ({ insight_id: ids[name], width: 6 }))
  const res = await api.post(`${base}/dashboards`, { data: { project_id: project, name: 'Layout', tiles } })
  expect(res.status()).toBe(201)
  return { ids, dashboard: ((await res.json()) as { id: string }).id }
}

test('edit a dashboard layout: width, reorder, add and remove, then save once', async ({
  page,
  playwright,
  world,
  baseURL,
}) => {
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const base = `/api/v1/workspaces/${workspace}/analytics`
  const api = await playwright.request.newContext({
    baseURL,
    extraHTTPHeaders: { authorization: `Bearer ${world.api_keys.full!.token}` },
  })
  try {
    const { ids, dashboard } = await setup(api, base, project, ['Sign-ins', 'Everyone', 'Signups'], 2)
    const errors = trackPageErrors(page)
    await page.goto(`/w/${workspace}/p/${project}/analytics/dashboards/${dashboard}`)
    await page.getByRole('button', { name: 'Edit layout' }).click()
    await expect(page.getByRole('region', { name: 'Everyone' }).getByRole('img')).toBeVisible()
    await expectCleanPage(page, errors)

    await page.getByRole('radiogroup', { name: 'Width of Sign-ins' }).getByRole('radio', { name: 'Full width' }).click()

    await page.getByRole('button', { name: 'Move Everyone', exact: true }).focus()
    await page.keyboard.press('ArrowLeft')
    await expect(page.getByRole('status').filter({ hasText: 'moved' })).toHaveText('Everyone moved to position 1 of 2')
    await expect(page.getByRole('button', { name: 'Move Everyone', exact: true })).toBeFocused()

    await page.getByRole('button', { name: 'Add insight' }).click()
    const panel = page.getByRole('dialog', { name: 'Add insights' })
    await expect(panel.getByRole('link', { name: 'New insight' })).toHaveAttribute(
      'href',
      new RegExp(`/insights/new\\?dashboard=${dashboard}$`),
    )
    await panel.getByText('Signups').click()
    await panel.getByRole('button', { name: 'Add', exact: true }).click()
    await expect(page.getByRole('region', { name: 'Signups' })).toBeVisible()

    await page.getByRole('button', { name: 'Remove Everyone', exact: true }).click()
    await expect(page.getByRole('region', { name: 'Everyone' })).toHaveCount(0)

    // Nothing is saved until Done.
    const before = (await (await api.get(`${base}/dashboards/${dashboard}`)).json()) as { tiles: Tile[] }
    expect(before.tiles).toHaveLength(2)

    await page.getByRole('button', { name: 'Done' }).click()
    await expect(page.getByRole('button', { name: 'Edit layout' })).toBeVisible()
    const after = (await (await api.get(`${base}/dashboards/${dashboard}`)).json()) as { tiles: Tile[] }
    expect(after.tiles).toEqual([
      { insight_id: ids['Sign-ins'], width: 12 },
      { insight_id: ids.Signups, width: 6 },
    ])
    await expect(page.getByRole('link', { name: 'Sign-ins' })).toBeVisible()
  } finally {
    await api.dispose()
  }
})

test('leaving with unsaved layout changes asks first', async ({ page, playwright, world, baseURL }) => {
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const base = `/api/v1/workspaces/${workspace}/analytics`
  const api = await playwright.request.newContext({
    baseURL,
    extraHTTPHeaders: { authorization: `Bearer ${world.api_keys.full!.token}` },
  })
  let dashboard = ''
  try {
    dashboard = (await setup(api, base, project, ['People'], 1)).dashboard
  } finally {
    await api.dispose()
  }
  await page.goto(`/w/${workspace}/p/${project}/analytics/dashboards/${dashboard}`)
  await page.getByRole('button', { name: 'Edit layout' }).click()
  await page.getByRole('radiogroup', { name: 'Width of People' }).getByRole('radio', { name: 'One third' }).click()

  const insightsTab = page.getByRole('link', { name: 'Insights', exact: true })
  const confirm = page.getByRole('dialog', { name: 'Discard layout changes?' })
  await insightsTab.click()
  await confirm.getByRole('button', { name: 'Cancel' }).click()
  await expect(page.getByRole('button', { name: 'Done' })).toBeVisible()

  await insightsTab.click()
  await confirm.getByRole('button', { name: 'Discard changes' }).click()
  await expect(page).toHaveURL(/\/analytics\/insights$/)
})
