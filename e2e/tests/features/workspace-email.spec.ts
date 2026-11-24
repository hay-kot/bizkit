import { MAILPIT_SMTP_PORT, MAILPIT_URL } from '../../fixtures/mailpit'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('an admin sends workspace mail through their own SMTP account', async ({ page, world, namespace }) => {
  const workspace = world.workspaces.main!.id
  const admin = world.account('admin').email
  const from = `news+${namespace}@tallyho.e2e.test`

  await page.goto(`/w/${workspace}/settings`)
  await page.getByRole('link', { name: 'Email', exact: true }).click()
  await expect(page.getByText(/through the instance mailer/)).toBeVisible()

  await page.getByLabel(/^From name/).fill('Tallyho')
  await page.getByLabel(/^From address/).fill(from)
  await page.getByLabel(/^Reply-To/).fill('help@tallyho.e2e.test')
  await page.getByLabel(/^Host/).fill('127.0.0.1')
  await page.getByLabel(/^Port/).fill(MAILPIT_SMTP_PORT)
  await page.getByRole('combobox', { name: 'Encryption' }).click()
  await page.getByRole('option', { name: /^None/ }).click()
  await page.getByRole('button', { name: 'Save changes' }).click()
  await expect(page.getByText('Mail settings saved')).toBeVisible()
  await expect(page.getByText('Not verified since the last change.')).toBeVisible()

  await page.getByRole('button', { name: 'Send test' }).click()
  await expect(page.getByText(`Sent to ${admin}. Check that inbox.`)).toBeVisible()
  await expect(page.getByText(/Verified .+\d{4}/)).toBeVisible()

  await expect
    .poll(async () => {
      const query = encodeURIComponent(`to:"${admin}" subject:"bizkit test mail"`)
      const found = await (await page.request.get(`${MAILPIT_URL}/api/v1/search?query=${query}`)).json()
      return found.messages?.[0]?.From?.Address ?? ''
    })
    .toBe(from)

  // A refused save keeps what the admin typed.
  await page.getByLabel(/^User name/).fill('someone')
  await page.getByRole('button', { name: 'Save changes' }).click()
  await expect(page.getByRole('alert').filter({ hasText: /password/i })).toBeVisible()
  await expect(page.getByLabel(/^User name/)).toHaveValue('someone')
  await page.getByLabel(/^User name/).fill('')
  await expect(page.getByRole('button', { name: 'Save changes' })).toBeDisabled()

  await page.getByRole('button', { name: 'Remove mail settings' }).click()
  await page.getByRole('button', { name: 'Remove', exact: true }).click()
  await expect(page.getByText('Mail settings removed')).toBeVisible()
  await expect(page.getByText(/through the instance mailer/)).toBeVisible()
})
