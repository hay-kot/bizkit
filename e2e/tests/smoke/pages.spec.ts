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
    await expect(page.getByRole('link', { name: /^Team/ })).toBeVisible()
    await expect(page.getByTestId('signed-in-as')).toHaveText(world.account('admin').email)
    await expectCleanPage(page, errors)
  })

  const workspacePages = ['projects', 'team', 'api-keys', 'settings']
  const projectPages = ['forms']
  for (const section of [...workspacePages, ...projectPages]) {
    test(`the ${section} page is clean`, async ({ page, world }) => {
      const errors = trackPageErrors(page)
      const workspace = `/w/${world.workspaces.main!.id}`
      await page.goto(
        projectPages.includes(section)
          ? `${workspace}/p/${world.projects.site!.id}/${section}`
          : `${workspace}/${section}`,
      )
      await expect(page.getByTestId('workspace-name')).toHaveText('Team')
      await expect(page.locator('.loading')).toHaveCount(0)
      await expectCleanPage(page, errors)
    })
  }

  test('a form page is clean', async ({ page, world, baseURL }) => {
    const errors = trackPageErrors(page)
    const workspace = world.workspaces.main!.id
    const created = await page.request.post(`/api/v1/workspaces/${workspace}/forms`, {
      data: { name: 'Contact', project_id: world.projects.site!.id, bot_check: 'challenge' },
      headers: { Origin: new URL(baseURL!).origin },
    })
    expect(created.status()).toBe(201)
    const form = await created.json()
    await page.request.post(form.endpoint, { form: { name: 'Ada' } })
    await page.goto(`/w/${workspace}/p/${world.projects.site!.id}/forms/${form.id}`)
    await expect(page.getByRole('heading', { name: 'Contact' })).toBeVisible()
    await page.getByText('HTML form').click()
    await expectCleanPage(page, errors)
  })
})
