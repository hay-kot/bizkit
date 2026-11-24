import { emailLinkPath } from '../../fixtures/mailpit'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('a contact confirms, gets a broadcast and unsubscribes through its link', async ({
  page,
  request,
  world,
  namespace,
}) => {
  const lea = `lea+${namespace}@e2e.test`
  const project = `/w/${world.workspaces.main!.id}/p/${world.projects.site!.id}`
  await page.goto(`${project}/mailing`)
  await page.getByRole('button', { name: 'New list' }).first().click()
  await page.getByLabel('Name').fill('Newsletter')
  await page.getByRole('button', { name: 'Create list' }).click()
  await expect(page.getByRole('heading', { name: 'Newsletter' })).toBeVisible()
  const listPath = new URL(page.url()).pathname

  await page.getByRole('button', { name: 'Add contact' }).click()
  await page.getByLabel('Email').fill(lea)
  await page.getByLabel('Name').fill('Lea')
  await page.getByRole('button', { name: 'Add contact' }).last().click()
  const row = page.getByRole('row', {
    name: new RegExp(lea.replace(/[+.]/g, '\\$&')),
  })
  await expect(row).toContainText('pending')

  // Opening the link changes nothing; the button on the page confirms.
  const confirm = await emailLinkPath(request, lea, /https?:\/\/\S+\/m\/mailing\/public\/confirm\?t=[\w-]+/)
  await page.goto(confirm)
  await page.getByRole('button', { name: 'Confirm subscription' }).click()
  await expect(page.getByRole('heading', { name: "You're subscribed" })).toBeVisible()

  await page.goto(listPath)
  await expect(row).toContainText('subscribed')

  await page.goto(`${project}/mailing/broadcasts`)
  await page.getByRole('button', { name: 'New broadcast' }).first().click()
  await page.getByLabel('Subject').fill(`Hello ${namespace}`)
  await page.getByRole('button', { name: 'Create draft' }).click()
  await expect(page.getByRole('heading', { name: `Hello ${namespace}` })).toBeVisible()
  await page.getByLabel('Body').fill('Hi {{ name }},\n\nWe shipped **CSV export**.')
  await page.getByRole('button', { name: 'Save' }).click()
  await expect(page.getByRole('button', { name: 'Saved' })).toBeVisible()
  await expect(page.frameLocator('iframe[title="Mail preview"]').getByText('CSV export')).toBeVisible()

  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await page.getByRole('button', { name: 'Send now' }).click()
  const log = page.getByRole('table', { name: 'Send log' })
  await expect(
    log.getByRole('row', {
      name: new RegExp(`${lea.replace(/[+.]/g, '\\$&')}.*sent`),
    }),
  ).toBeVisible({
    timeout: 15_000,
  })

  const unsubscribe = await emailLinkPath(request, lea, /https?:\/\/\S+\/m\/mailing\/public\/unsubscribe\?t=[\w-]+/)
  await page.goto(unsubscribe)
  await page.getByRole('button', { name: 'Unsubscribe' }).click()
  await expect(page.getByRole('heading', { name: "You're unsubscribed" })).toBeVisible()

  await page.goto(listPath)
  await expect(row).toContainText('unsubscribed')
})
