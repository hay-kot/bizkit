import { type APIRequestContext } from '@playwright/test'

import { MAILPIT_URL } from '../../fixtures/mailpit'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

/** Deliveries of a broadcast that fail in a row before it pauses. */
const BREAKER_FAILURES = 20

function chaos(request: APIRequestContext, probability: number) {
  return request.put(`${MAILPIT_URL}/api/v1/chaos`, {
    data: { Recipient: { ErrorCode: 451, Probability: probability } },
  })
}

test('a broadcast pauses after 20 failures and resume sends everyone once', async ({
  page,
  request,
  world,
  baseURL,
  namespace,
}) => {
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const headers = { Origin: new URL(baseURL!).origin }
  const api = `/api/v1/workspaces/${workspace}/mailing`
  const created = await page.request.post(`${api}/lists`, {
    data: { name: `Breaker ${namespace}`, project_id: project, double_opt_in: false },
    headers,
  })
  const list = await created.json()
  const recipients = BREAKER_FAILURES + 1
  for (let i = 0; i < recipients; i++) {
    const added = await page.request.post(`${api}/lists/${list.id}/contacts`, {
      data: { email: `c${i}+${namespace}@e2e.test` },
      headers,
    })
    expect(added.status()).toBe(201)
  }
  const drafted = await page.request.post(`${api}/broadcasts`, {
    data: { list_id: list.id, subject: `Breaker ${namespace}`, body_md: 'Hi' },
    headers,
  })
  const broadcast = await drafted.json()

  expect((await chaos(request, 100)).ok()).toBe(true)
  try {
    const sent = await page.request.post(`${api}/broadcasts/${broadcast.id}/send`, { data: {}, headers })
    expect(sent.status()).toBe(200)
    // Runs the fan-out and every first attempt; retries wait for the clock.
    for (let i = 0; i < 3; i++) expect((await request.post('/__test/jobs/drain')).ok()).toBe(true)
  } finally {
    await chaos(request, 0)
  }

  await page.goto(`/w/${workspace}/p/${project}/mailing/broadcasts/${broadcast.id}`)
  await expect(page.getByRole('alert')).toContainText('20 deliveries failed in a row')
  await expect(page.locator('.badge', { hasText: 'paused' }).first()).toBeVisible()
  await page.getByRole('button', { name: 'Resume', exact: true }).click()
  await page.getByRole('dialog').getByRole('button', { name: 'Resume', exact: true }).click()
  await expect(page.getByText('Resumed')).toBeVisible()

  for (let i = 0; i < 10; i++) {
    expect((await request.post('/__test/clock', { data: { advance_ms: 3_600_000 } })).ok()).toBe(true)
    expect((await request.post('/__test/jobs/drain')).ok()).toBe(true)
  }
  const log = await page.request.get(`${api}/deliveries?broadcast_id=${broadcast.id}&limit=50`)
  const items: { status: string; to_email: string }[] = (await log.json()).items
  expect(items).toHaveLength(recipients)
  expect(items.every((d) => d.status === 'sent')).toBe(true)
  const inbox = await request.get(
    `${MAILPIT_URL}/api/v1/search?query=${encodeURIComponent(`subject:"Breaker ${namespace}"`)}&limit=100`,
  )
  expect((await inbox.json()).messages).toHaveLength(recipients)
})
