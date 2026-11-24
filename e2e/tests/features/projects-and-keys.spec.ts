import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('create a project', async ({ page, world }) => {
  await page.goto(`/w/${world.workspaces.main!.id}/projects`)
  await expect(page.getByRole('cell', { name: 'Website', exact: true })).toBeVisible()
  await page.getByRole('button', { name: 'New project' }).click()
  await page.getByLabel('Name').fill('Docs')
  await expect(page.getByLabel('Slug')).toHaveValue('docs')
  await page.getByRole('button', { name: 'Create project' }).click()
  await expect(page.getByRole('cell', { name: 'Docs', exact: true })).toBeVisible()
})

test('the sidebar puts project pages under the project and workspace meta at the bottom', async ({
  page,
  world,
}) => {
  const workspace = `/w/${world.workspaces.main!.id}`
  await page.goto(workspace)
  // With nothing remembered, the first project by name.
  await expect(page).toHaveURL(new RegExp(`${workspace}/p/${world.projects.app!.id}/forms$`))
  await expect(page.getByTestId('project-name')).toHaveText('App')
  await expect(
    page.getByRole('navigation', { name: 'Project' }).getByRole('link', { name: 'Forms' }),
  ).toBeVisible()
  await expect(
    page.getByRole('navigation', { name: 'Workspace' }).getByRole('link'),
  ).toHaveText(['Team', 'API keys', 'Settings'])
})

test('a link to an unknown project says so', async ({ page, world }) => {
  await page.goto(`/w/${world.workspaces.main!.id}/p/00000000-0000-7000-8000-000000000000/forms`)
  await expect(page.getByText('Project not found')).toBeVisible()
  await expect(page.getByTestId('project-name')).toHaveText('Choose a project')
})

test('create an API key and see its token once', async ({ page, world }) => {
  await page.goto(`/w/${world.workspaces.main!.id}/api-keys`)
  await page.getByRole('button', { name: 'New API key' }).click()
  await page.getByLabel('Name').fill('CI')
  await page.locator('input[value="projects:read"]').check()
  await page.getByRole('button', { name: 'Create key' }).click()
  const token = page.getByTestId('token-value')
  await expect(token).toHaveText(/^bk_[\w-]{43}$/)
  const prefix = (await token.textContent())!.slice(0, 10)
  await expect(page.getByRole('cell', { name: 'CI', exact: true })).toBeVisible()
  await page.getByRole('button', { name: 'Done' }).click()
  await expect(page.getByTestId('new-token')).toHaveCount(0)

  await page.reload()
  await expect(page.getByTestId('new-token')).toHaveCount(0)
  await expect(page.getByRole('cell', { name: `${prefix}…` })).toBeVisible()
})

test.describe('as a member', () => {
  test.use({ as: 'member' })

  test('admin sections are hidden', async ({ page, world }) => {
    await page.goto(`/w/${world.workspaces.main!.id}/team`)
    const sidebar = page.getByRole('navigation', { name: 'Workspace', exact: true })
    // The breadcrumb links the workspace, which this persona names "Team".
    await expect(sidebar.getByRole('link', { name: 'Team' })).toBeVisible()
    await expect(page.getByRole('link', { name: 'API keys' })).toHaveCount(0)
    await expect(page.getByRole('link', { name: 'Settings' })).toHaveCount(0)
    await expect(page.getByRole('heading', { name: 'Pending invitations' })).toHaveCount(0)
  })
})
