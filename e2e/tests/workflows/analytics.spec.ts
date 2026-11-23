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
