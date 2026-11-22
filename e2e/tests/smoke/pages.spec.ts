import { expectCleanPage, trackPageErrors } from '../../fixtures/clean-page'
import { expect, test } from '../../fixtures/test'

test.describe('signed out', () => {
  test('the sign-in page is clean', async ({ page }) => {
    const errors = trackPageErrors(page)
    await page.goto('/sign-in')
    await expect(page.getByRole('heading', { name: 'Sign in to bizkit' })).toBeVisible()
    await expectCleanPage(page, errors)
  })

  test('app pages send you to sign in', async ({ page }) => {
    await page.goto('/')
    await expect(page).toHaveURL(/\/sign-in\?next=(?:%2F|\/)$/)
  })
})

test.describe('workspace pages', () => {
  test.use({ persona: 'team' })

  test('the workspace list is clean', async ({ page, world }) => {
    const errors = trackPageErrors(page)
    await page.goto('/')
    await expect(page.getByRole('link', { name: 'Team' })).toBeVisible()
    await expect(page.getByTestId('signed-in-as')).toHaveText(world.account('admin').email)
    await expectCleanPage(page, errors)
  })

  for (const section of ['projects', 'members', 'invitations', 'api-keys', 'settings']) {
    test(`the ${section} page is clean`, async ({ page, world }) => {
      const errors = trackPageErrors(page)
      await page.goto(`/w/${world.workspaces.main!.id}/${section}`)
      await expect(page.getByTestId('workspace-name')).toHaveText('Team')
      await expect(page.locator('.loading')).toHaveCount(0)
      await expectCleanPage(page, errors)
    })
  }
})
