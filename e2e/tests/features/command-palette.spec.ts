import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('the palette goes to a form and opens the New form panel', async ({ page, world, baseURL }) => {
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const created = await page.request.post(`/api/v1/workspaces/${workspace}/forms`, {
    data: { name: 'Contact', project_id: project },
    headers: { Origin: new URL(baseURL!).origin },
  })
  expect(created.status()).toBe(201)
  const form = await created.json()

  await page.goto(`/w/${workspace}/p/${project}/webhooks`)
  await expect(page.getByTestId('workspace-name')).toHaveText('Team')
  await page.keyboard.press('Control+k')
  const palette = page.getByRole('dialog', { name: 'Command palette' })
  await expect(palette).toBeVisible()
  await palette.getByRole('combobox', { name: 'Search' }).fill('contact')
  await expect(palette.getByRole('option', { name: /Contact/ })).toHaveAttribute('aria-selected', 'true')
  await page.keyboard.press('Enter')
  await expect(page).toHaveURL(new RegExp(`/forms/${form.id}$`))
  await expect(page.getByRole('heading', { name: 'Contact' })).toBeVisible()
  await expect(palette).toHaveCount(0)

  await page.getByTestId('open-palette').click()
  await palette.getByRole('combobox', { name: 'Search' }).pressSequentially('>new form')
  await expect(palette.getByRole('tab', { name: /Actions/ })).toHaveAttribute('aria-selected', 'true')
  await page.keyboard.press('Enter')
  await expect(page).toHaveURL(new RegExp(`/p/${project}/forms$`))
  await expect(page.getByRole('dialog', { name: 'New form' })).toBeVisible()
})
