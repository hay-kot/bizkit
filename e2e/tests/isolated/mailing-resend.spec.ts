import { type APIRequestContext } from '@playwright/test'

import { MAILPIT_URL } from '../../fixtures/mailpit'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

function chaos(request: APIRequestContext, probability: number) {
  return request.put(`${MAILPIT_URL}/api/v1/chaos`, {
    data: { Recipient: { ErrorCode: 451, Probability: probability } },
  })
}

/** Moves the server clock past every retry backoff and runs the due jobs. */
async function runRetries(request: APIRequestContext) {
  for (let i = 0; i < 10; i++) {
    expect((await request.post('/__test/clock', { data: { advance_ms: 3_600_000 } })).ok()).toBe(true)
    expect((await request.post('/__test/jobs/drain')).ok()).toBe(true)
  }
}

test('a broadcast resends only to the recipient whose mail failed', async ({
  page,
  request,
  world,
  baseURL,
  namespace,
}) => {
  const workspace = world.workspaces.main!.id
  const headers = { Origin: new URL(baseURL!).origin }
  const zoe = `zoe+${namespace}@e2e.test`
  const created = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/lists`, {
    data: { name: 'Newsletter', project_id: world.projects.site!.id, double_opt_in: false },
    headers,
  })
  expect(created.status()).toBe(201)
  const list = await created.json()
  const contacts = `/api/v1/workspaces/${workspace}/mailing/lists/${list.id}/contacts`
  expect((await page.request.post(contacts, { data: { email: zoe }, headers })).status()).toBe(201)
  const drafted = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/broadcasts`, {
    data: { list_id: list.id, subject: `Resend ${namespace}`, body_md: 'Hi' },
    headers,
  })
  const broadcast = await drafted.json()

  await page.goto(`/w/${workspace}/p/${world.projects.site!.id}/mailing/broadcasts/${broadcast.id}`)
  const row = page
    .getByRole('table', { name: 'Send log' })
    .getByRole('row', { name: new RegExp(zoe.replace(/[+.]/g, '\\$&')) })
  expect((await chaos(request, 100)).ok()).toBe(true)
  try {
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    await page.getByRole('button', { name: 'Send now' }).click()
    await expect(page.getByRole('heading', { name: 'Send log' })).toBeVisible()
    await runRetries(request)
  } finally {
    await chaos(request, 0)
  }

  await page.reload()
  await expect(row).toContainText('failed')
  await page.getByRole('button', { name: 'Resend to 1 failed' }).click()
  await page.getByRole('button', { name: 'Resend', exact: true }).click()
  await expect(page.getByText('Queued 1 again')).toBeVisible()
  await request.post('/__test/jobs/drain')

  await expect(row).toContainText('sent')
  await expect(row).toContainText('resent')
  await expect(page.getByRole('button', { name: 'Resend to failed' })).toBeDisabled()
  const inbox = await request.get(`${MAILPIT_URL}/api/v1/search?query=${encodeURIComponent(`to:"${zoe}"`)}`)
  expect((await inbox.json()).messages).toHaveLength(1)
})
