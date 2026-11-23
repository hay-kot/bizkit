import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('a service posts to a new endpoint; the admin inspects the request', async ({
  page,
  request,
  world,
}) => {
  await page.goto(`/w/${world.workspaces.main!.id}/p/${world.projects.site!.id}/webhooks`)
  await page.getByRole('button', { name: 'New endpoint' }).first().click()
  await page.getByLabel('Name').fill('Billing')
  await page.getByRole('button', { name: 'Create endpoint' }).click()
  await expect(page.getByRole('heading', { name: 'Billing' })).toBeVisible()
  await expect(page.getByText('No requests yet')).toBeVisible()

  const url = (await page.getByTestId('endpoint-url').textContent())!
  const posted = await request.post(`${url}?attempt=1`, {
    data: { type: 'invoice.paid', amount: 4900 },
    headers: { 'stripe-signature': 't=1,v1=abc', authorization: 'Bearer secret' },
  })
  expect(posted.status()).toBe(200)
  expect(await posted.json()).toEqual({ ok: true })

  await page.getByRole('button', { name: 'Refresh' }).click()
  const row = page.getByRole('row', { name: /POST\s*\?attempt=1/ })
  await expect(row).toBeVisible()
  await row.getByRole('button', { name: /^Inspect the POST request/ }).click()

  const panel = page.getByRole('dialog')
  await expect(panel.getByText('stripe-signature:')).toBeVisible()
  await expect(panel.getByText('authorization:')).toHaveCount(0)
  await expect(panel.getByText('"invoice.paid"')).toBeVisible()
  await panel.getByRole('button', { name: 'Close' }).click()

  await page.getByRole('button', { name: 'Settings' }).click()
  await page.getByLabel('Accept requests').uncheck()
  await page.getByRole('button', { name: 'Save settings' }).click()
  await expect(page.getByText('Disabled: requests get a 404')).toBeVisible()
  expect((await request.post(url, { data: {} })).status()).toBe(404)
})
