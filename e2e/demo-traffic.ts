// Drives headless visitors through the dev stack's demo site so the Website
// project's funnels fill from real ingest. Run it with
// `mise run demo:traffic -- --site <demo site url> --api <api url>`.
//
// The pages and the snippet run for real. Only the visitor's address is made
// up: beacons go straight to the API with a per-visitor X-Forwarded-For,
// because through the Vite proxy every visitor would be 127.0.0.1 and hit the
// per-client ingest limit.
import { chromium, type Browser, type Locator, type Page } from '@playwright/test'
import { parseArgs } from 'node:util'

const { values: args } = parseArgs({
  options: {
    site: { type: 'string' },
    api: { type: 'string' },
    visitors: { type: 'string', default: '200' },
    concurrency: { type: 'string', default: '4' },
  },
})

if (!args.site || !args.api) {
  console.error('usage: demo-traffic --site <demo site url> --api <api url> [--visitors 200] [--concurrency 4]')
  process.exit(2)
}
const site = new URL(args.site)
const api = new URL(args.api)
const visitors = Number(args.visitors)
const concurrency = Number(args.concurrency)
if (!Number.isInteger(visitors) || visitors < 1 || !Number.isInteger(concurrency) || concurrency < 1) {
  console.error('--visitors and --concurrency must be positive integers')
  process.exit(2)
}
if (site.hostname !== 'localhost') {
  // The demo site's hostnames list `localhost` only; other hosts are dropped.
  console.error('--site must be the http://localhost:<port> URL from `svx status`')
  process.exit(2)
}

type Agent = { userAgent: string; viewport: { width: number; height: number } }

const desktop = { width: 1440, height: 900 }
const phone = { width: 390, height: 844 }
const agents: [number, Agent][] = [
  [24, { viewport: desktop, userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36' }],
  [18, { viewport: desktop, userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36' }],
  [12, { viewport: desktop, userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15' }],
  [6, { viewport: desktop, userAgent: 'Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0' }],
  [6, { viewport: desktop, userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0' }],
  [18, { viewport: phone, userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1' }],
  [16, { viewport: phone, userAgent: 'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36' }],
]

type Source = { referer?: string; query?: string }
const sources: [number, Source][] = [
  [35, {}],
  [25, { referer: 'https://www.google.com/' }],
  [10, { referer: 'https://news.ycombinator.com/' }],
  [8, { referer: 'https://t.co/', query: 'utm_source=twitter&utm_medium=social&utm_campaign=launch' }],
  [7, { referer: 'https://github.com/' }],
  [8, { query: 'utm_source=newsletter&utm_medium=email&utm_campaign=november' }],
  [7, { referer: 'https://duckduckgo.com/' }],
]

function pick<T>(choices: [number, T][]): T {
  let roll = Math.random() * choices.reduce((sum, [weight]) => sum + weight, 0)
  for (const [weight, choice] of choices) {
    roll -= weight
    if (roll < 0) return choice
  }
  return choices[choices.length - 1]![1]
}

const chance = (percent: number) => Math.random() * 100 < percent
const think = (page: Page) => page.waitForTimeout(300 + Math.random() * 1200)

// A click that waits at most 5 s and tries once more, because a page now and
// then misses its first click while it is still loading (and a sleeping host
// can stall one past any timeout).
async function press(target: Locator) {
  try {
    await target.click({ timeout: 5_000 })
  } catch {
    await target.click({ timeout: 5_000 })
  }
}

// A random address outside the private and reserved ranges, so each visitor
// is its own client to the rate limit and the visitor hash.
function publicIp(): string {
  const first = pick<number>([
    [1, 24],
    [1, 51],
    [1, 73],
    [1, 81],
    [1, 92],
    [1, 103],
    [1, 151],
    [1, 185],
  ])
  const byte = () => 1 + Math.floor(Math.random() * 254)
  return `${first}.${byte()}.${byte()}.${byte()}`
}

const stats = { visitors: 0, pricing: 0, signupStarted: 0, signedUp: 0, projects: 0, invites: 0, reports: 0, beacons: 0, rejected: 0 }

// One visit, with drop-off rates near the demo seed's so live traffic and
// history tell the same story.
async function walk(page: Page) {
  const source = pick(sources)
  const landing = new URL('/', site)
  if (source.query) landing.search = source.query
  await page.goto(landing.href, { referer: source.referer })
  await think(page)

  if (chance(25)) {
    await press(page.getByRole('link', { name: 'Start free' }))
  } else {
    if (!chance(55)) return
    await press(page.getByRole('link', { name: 'See pricing' }))
    stats.pricing++
    await think(page)
    if (!chance(45)) return
    await press(page.getByRole('link', { name: chance(30) ? 'Start Pro' : 'Start free' }))
  }
  await page.waitForURL('**/signup**')
  await think(page)
  if (!chance(85)) return
  await press(page.getByLabel('Name'))
  stats.signupStarted++
  await think(page)
  if (!chance(60)) return
  await press(page.getByRole('button', { name: 'Sign up' }))
  stats.signedUp++

  await page.waitForURL('**/onboarding/project')
  await think(page)
  if (!chance(80)) return
  await press(page.getByRole('button', { name: 'Create project' }))
  stats.projects++

  await page.waitForURL('**/onboarding/invite')
  await think(page)
  if (chance(55)) {
    await press(page.getByRole('button', { name: 'Send invite' }))
    stats.invites++
  } else if (chance(50)) {
    await press(page.getByRole('link', { name: 'Skip for now' }))
  } else {
    return
  }

  await page.waitForURL('**/onboarding/report')
  await think(page)
  if (!chance(70)) return
  await press(page.getByRole('button', { name: 'Open the weekly report' }))
  stats.reports++
  await page.waitForURL('**/app')
}

async function visit(browser: Browser) {
  const agent = pick(agents)
  const ip = publicIp()
  const context = await browser.newContext(agent)
  // The snippet skips automated browsers and prefers sendBeacon, which
  // request routing doesn't see. Look like a person and fall back to fetch.
  await context.addInitScript(() => {
    Object.defineProperty(Navigator.prototype, 'webdriver', { get: () => false })
    // @ts-expect-error -- removing the method makes the snippet use fetch
    delete Navigator.prototype.sendBeacon
  })
  let pending = 0
  await context.route('**/m/analytics/public/e', async (route) => {
    pending++
    try {
      const response = await route.fetch({
        url: new URL('/m/analytics/public/e', api).href,
        headers: { ...route.request().headers(), 'x-forwarded-for': ip },
      })
      stats.beacons++
      if (response.status() !== 202) stats.rejected++
      await route.fulfill({ response })
    } catch {
      stats.rejected++
      await route.abort().catch(() => {})
    } finally {
      pending--
    }
  })
  const page = await context.newPage()
  page.setDefaultTimeout(10_000)
  try {
    await walk(page)
    // Let the last beacon land before the context closes.
    await page.waitForTimeout(300)
    while (pending > 0) await page.waitForTimeout(50)
  } finally {
    await context.close()
  }
  stats.visitors++
}

const browser = await chromium.launch()
let next = 0
let failed = 0
async function worker() {
  while (next < visitors) {
    next++
    try {
      await visit(browser)
    } catch (error) {
      failed++
      console.error(`visit failed: ${error instanceof Error ? error.message : String(error)}`)
    }
    const done = stats.visitors + failed
    if (done % 25 === 0) console.error(`${done}/${visitors} visits`)
  }
}
await Promise.all(Array.from({ length: Math.min(concurrency, visitors) }, worker))
await browser.close()

console.log(
  [
    `visitors        ${stats.visitors} (${failed} failed)`,
    `pricing         ${stats.pricing}`,
    `signup started  ${stats.signupStarted}`,
    `signed up       ${stats.signedUp}`,
    `project created ${stats.projects}`,
    `invite sent     ${stats.invites}`,
    `report viewed   ${stats.reports}`,
    `beacons         ${stats.beacons} (${stats.rejected} not accepted)`,
  ].join('\n'),
)
if (failed > 0 || stats.rejected > 0) process.exitCode = 1
