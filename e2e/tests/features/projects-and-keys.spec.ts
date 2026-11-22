import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('create a project', async ({ page, world }) => {
  await page.goto(`/w/${world.workspaces.main!.id}/projects`)
  await expect(page.getByRole('cell', { name: 'Website', exact: true })).toBeVisible()
  await page.getByLabel('Name').fill('Docs')
  await expect(page.getByLabel('Slug')).toHaveValue('docs')
  await page.getByRole('button', { name: 'Create project' }).click()
  await expect(page.getByRole('cell', { name: 'Docs', exact: true })).toBeVisible()
})

test('create an API key and see its token once', async ({ page, world }) => {
  await page.goto(`/w/${world.workspaces.main!.id}/api-keys`)
  await page.getByLabel('Name').fill('CI')
  await page.locator('input[value="projects:read"]').check()
  await page.getByRole('button', { name: 'Create key' }).click()
  const token = page.getByTestId('new-token').locator('code')
  await expect(token).toHaveText(/^bk_[\w-]{43}$/)
  const prefix = (await token.textContent())!.slice(0, 10)
  await expect(page.getByRole('cell', { name: 'CI', exact: true })).toBeVisible()

  await page.reload()
  await expect(page.getByTestId('new-token')).toHaveCount(0)
  await expect(page.getByRole('cell', { name: `${prefix}…` })).toBeVisible()
})

test.describe('as a member', () => {
  test.use({ as: 'member' })

  test('admin sections are hidden', async ({ page, world }) => {
    await page.goto(`/w/${world.workspaces.main!.id}/projects`)
    await expect(page.getByRole('link', { name: 'Members' })).toBeVisible()
    await expect(page.getByRole('link', { name: 'API keys' })).toHaveCount(0)
  })
})
