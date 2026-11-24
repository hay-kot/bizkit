import { expectCleanPage, trackPageErrors } from '../../fixtures/clean-page'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

function utcDay(offset: number): string {
  return new Date(Date.now() + offset * 86_400_000).toISOString().slice(0, 10)
}

test('the time range shifts, zooms out and picks quick ranges through the URL', async ({
  page,
  world,
  baseURL,
}) => {
  const errors = trackPageErrors(page)
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const created = await page.request.post(`/api/v1/workspaces/${workspace}/analytics/site`, {
    data: { project_id: project },
    headers: { Origin: new URL(baseURL!).origin },
  })
  expect(created.status()).toBe(201)
  const site = (await created.json()) as { key: string }
  const beacon = await page.request.post('/m/analytics/public/e', {
    data: { s: site.key, n: 'pageview', u: 'http://site.e2e.test/', w: 1440 },
  })
  expect(beacon.ok()).toBe(true)
  // Events land after the writer's flush window; open the page once they have.
  const hasEvents = async () => {
    const res = await page.request.get(`/api/v1/workspaces/${workspace}/analytics/site?project_id=${project}`)
    return ((await res.json()) as { has_events: boolean }).has_events
  }
  await expect.poll(hasEvents).toBe(true)

  await page.goto(`/w/${workspace}/p/${project}/analytics?range=7d`)
  const trigger = page.getByRole('button', { name: 'Time range: Last 7 days' })
  await expect(trigger).toBeVisible()
  await expect(page.getByRole('button', { name: 'Next period', exact: true })).toBeDisabled()

  await page.getByRole('button', { name: 'Previous period', exact: true }).click()
  await expect(page).toHaveURL(new RegExp(`range=custom&from=${utcDay(-13)}&to=${utcDay(-7)}`))
  await expect(page.getByRole('button', { name: 'Next period', exact: true })).toBeEnabled()

  await page.getByRole('button', { name: 'Zoom out' }).click()
  await expect(page).toHaveURL(new RegExp(`from=${utcDay(-17)}&to=${utcDay(-4)}`))

  await page.getByRole('button', { name: /^Time range:/ }).click()
  const dialog = page.getByRole('dialog', { name: 'Time range' })
  await expect(dialog).toBeVisible()
  await expectCleanPage(page, errors)
  await dialog.getByRole('button', { name: 'Last 14 days' }).click()
  await expect(page).toHaveURL(/range=14d/)
  await expect(dialog).toBeHidden()

  await page.getByRole('button', { name: 'Time range: Last 14 days' }).click()
  await dialog.getByLabel('From').fill(utcDay(-3))
  await dialog.getByLabel('To').fill(utcDay(-1))
  await dialog.getByLabel('To').press('Enter')
  await expect(page).toHaveURL(new RegExp(`range=custom&from=${utcDay(-3)}&to=${utcDay(-1)}`))

  await page.getByRole('button', { name: /^Time range:/ }).click()
  await page.keyboard.press('Escape')
  await expect(dialog).toBeHidden()
  await expect(page.getByRole('button', { name: /^Time range:/ })).toBeFocused()

  const compare = page.getByRole('button', { name: 'Compare to previous period' })
  await compare.click()
  await expect(compare).toHaveAttribute('aria-pressed', 'true')
  await expect(page).toHaveURL(/compare=1/)
})
