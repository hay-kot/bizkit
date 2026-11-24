import { createServer, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'

import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

// The customer's site: a real loopback server on its own port, so the
// snippet runs cross-origin. Chromium treats a routed page as public and
// blocks its requests to the loopback test server.
async function serveSite(html: () => string): Promise<{ url: string; server: Server }> {
  const server = createServer((_, res) => {
    res.writeHead(200, { 'content-type': 'text/html' })
    res.end(html())
  })
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  return { url: `http://127.0.0.1:${(server.address() as AddressInfo).port}/`, server }
}

test('an admin turns on analytics; a cookieless site sends a pageview that the overview shows', async ({
  page,
  context,
  world,
}) => {
  await page.goto(`/w/${world.workspaces.main!.id}/p/${world.projects.site!.id}/analytics`)
  await page.getByRole('button', { name: 'Turn on analytics' }).click()
  const snippet = (await page.getByTestId('analytics-snippet').textContent())!
  expect(snippet).toMatch(/^<script defer src="[^"]+\/m\/analytics\/public\/s\/[\w-]+\.js"><\/script>$/)

  const { url, server } = await serveSite(
    () => `<!doctype html><html><head><title>Site</title>${snippet}</head><body>Hello</body></html>`,
  )
  try {
    const site = await context.newPage()
    // The snippet ignores automated browsers, so this page pretends not to be one.
    await site.addInitScript(() => Object.defineProperty(navigator, 'webdriver', { get: () => false }))
    const beacon = site.waitForRequest((r) => r.url().endsWith('/m/analytics/public/e'))
    await site.goto(`${url}pricing?utm_source=newsletter`)
    expect(JSON.parse((await beacon).postData()!)).toMatchObject({ n: 'pageview' })
    expect(await site.evaluate(() => [localStorage.length, document.cookie])).toEqual([0, ''])
  } finally {
    server.close()
  }

  await expect(page.getByRole('heading', { name: 'Top pages' })).toBeVisible({ timeout: 15_000 })
  const pages = page.getByRole('region', { name: 'Top pages' })
  await expect(pages.getByRole('listitem')).toHaveText(['/pricing Pageviews: 1 Visitors: 1'], {
    useInnerText: true,
  })
})

test('a funnel lists the people who dropped off before a step', async ({ page, playwright, world, baseURL }) => {
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
      { name: '$identify', user_id: 'ada', traits: { email: 'ada@example.com' } },
      { name: 'pageview', user_id: 'ada', url: 'https://x.test/pricing' },
      { name: 'signup_started', user_id: 'ada' },
      { name: 'signup', user_id: 'ada' },
      { name: '$identify', user_id: 'bob', traits: { email: 'bob@example.com' } },
      { name: 'pageview', user_id: 'bob', url: 'https://x.test/pricing' },
      { name: 'signup_started', user_id: 'bob' },
    ]
    expect((await api.post(`${base}/events`, { data: { project_id: project, events } })).status()).toBe(200)
    const steps = [
      { kind: 'page', match: '/pricing' },
      { kind: 'event', match: 'signup_started' },
      { kind: 'event', match: 'signup' },
    ]
    const created = await api.post(`${base}/funnels`, { data: { project_id: project, name: 'Signup', steps } })
    expect(created.status()).toBe(201)
    const funnel = (await created.json()) as { id: string }

    // Events land after the writer's flush window; open the page once they have.
    const hasEvents = async () => {
      const site = (await (await api.get(`${base}/site?project_id=${project}`)).json()) as { has_events: boolean }
      return site.has_events
    }
    await expect.poll(hasEvents).toBe(true)
    await page.goto(`/w/${workspace}/p/${project}/analytics/funnels/${funnel.id}`)
    await page.getByRole('link', { name: '1 dropped off' }).click()
    await expect(page.getByRole('heading', { name: 'Dropped before step 3: signup' })).toBeVisible()
    const table = page.getByRole('table', { name: 'People who dropped off' })
    await expect(table.getByRole('link', { name: 'bob@example.com' })).toBeVisible()
    await expect(table).not.toContainText('ada@example.com')
    await page.getByRole('link', { name: 'Close' }).click()
    await expect(table).toBeHidden()
  } finally {
    await api.dispose()
  }
})
