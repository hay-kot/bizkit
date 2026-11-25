import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('an admin turns a feature off and on again from Settings', async ({ page, world, baseURL }) => {
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const sidebar = page.getByRole('navigation', { name: 'Project' })
  const forms = page.getByRole('checkbox', { name: /^Forms/ })

  await page.goto(`/w/${workspace}/settings`)
  await expect(sidebar.getByRole('link', { name: 'Forms' })).toBeVisible()
  await expect(forms).toBeChecked()
  await forms.uncheck()
  await page.getByRole('button', { name: 'Save features' }).click()
  await expect(page.getByText('Features saved')).toBeVisible()
  await expect(sidebar.getByRole('link', { name: 'Forms' })).toBeHidden()

  // Opening the project lands on its first page that is on.
  await page.goto(`/w/${workspace}/p/${project}`)
  await expect(page).toHaveURL(new RegExp(`/p/${project}/analytics`))

  await page.goto(`/w/${workspace}/p/${project}/forms`)
  await expect(page.getByText('Forms is off', { exact: true })).toBeVisible()
  const refused = await page.request.get(`/api/v1/workspaces/${workspace}/forms?project_id=${project}`, {
    headers: { Origin: new URL(baseURL!).origin },
  })
  expect(refused.status()).toBe(403)

  await page.getByRole('link', { name: 'Open settings' }).click()
  await forms.check()
  await page.getByRole('button', { name: 'Save features' }).click()
  await expect(page.getByText('Features saved')).toBeVisible()
  await sidebar.getByRole('link', { name: 'Forms' }).click()
  await expect(page.getByText('Forms is off', { exact: true })).toBeHidden()
  await expect(page.getByRole('heading', { name: 'Forms', exact: true })).toBeVisible()
})
