import { emailLinkPath } from '../../fixtures/mailpit'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('a site posts to a new form; the admin gets mail, flags spam and exports', async ({
  page,
  request,
  world,
  namespace,
}) => {
  const owner = `owner+${namespace}@e2e.test`
  await page.goto(`/w/${world.workspaces.main!.id}/p/${world.projects.site!.id}/forms`)
  await page.getByRole('button', { name: 'New form' }).first().click()
  await page.getByLabel('Name').fill('Contact')
  await page.getByRole('button', { name: 'Create form' }).click()
  await expect(page.getByRole('heading', { name: 'Contact' })).toBeVisible()

  await page.getByRole('button', { name: 'Settings' }).click()
  await page.getByLabel('Notify').fill(owner)
  await page.getByRole('button', { name: 'Save settings' }).click()
  await expect(page.getByRole('dialog')).toHaveCount(0)

  // A static site's plain HTML form post.
  const endpoint = (await page.getByTestId('form-endpoint').textContent())!
  const posted = await request.post(endpoint, {
    form: { name: 'Ada', message: 'Hello from the site', _gotcha: '' },
    maxRedirects: 0,
  })
  expect(posted.status()).toBe(200)
  expect(await posted.text()).toContain('Thanks!')

  await page.reload()
  const row = page.getByRole('row', { name: /Hello from the site · Ada/ })
  await expect(row).toBeVisible()

  const link = await emailLinkPath(request, owner, /https?:\/\/\S+\/w\/[0-9a-f-]+\/p\/[0-9a-f-]+\/forms\/[0-9a-f-]+/)
  expect(link).toBe(new URL(page.url()).pathname)

  await row.getByRole('button', { name: 'Mark as spam' }).click()
  await expect(page.getByText('Moved to spam')).toBeVisible()
  await expect(row).toHaveCount(0)
  await page.getByRole('combobox', { name: 'Show' }).click()
  await page.getByRole('option', { name: 'Spam' }).click()
  await expect(page.getByRole('row', { name: /Hello from the site · Ada/ })).toBeVisible()

  const download = page.waitForEvent('download')
  await page.getByRole('button', { name: 'Export CSV' }).click()
  expect((await download).suggestedFilename()).toMatch(/^contact-spam-\d{4}-\d{2}-\d{2}\.csv$/)
})
