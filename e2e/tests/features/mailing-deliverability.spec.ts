import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('a broadcast is scheduled, then cancelled back to a draft', async ({ page, world, baseURL, namespace }) => {
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const headers = { Origin: new URL(baseURL!).origin }
  const created = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/lists`, {
    data: { name: `Schedule ${namespace}`, project_id: project, double_opt_in: false },
    headers,
  })
  const list = await created.json()
  const drafted = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/broadcasts`, {
    data: { list_id: list.id, subject: `Later ${namespace}`, body_md: 'Hello' },
    headers,
  })
  const broadcast = await drafted.json()

  await page.goto(`/w/${workspace}/p/${project}/mailing/broadcasts/${broadcast.id}`)
  await page.getByRole('button', { name: 'Schedule', exact: true }).click()
  const dialog = page.getByRole('dialog', { name: 'Schedule broadcast' })
  await expect(dialog.getByLabel('Send at')).not.toHaveValue('')
  await dialog.getByRole('button', { name: 'Schedule', exact: true }).click()
  await expect(page.getByText(/^Scheduled for/)).toBeVisible()
  await expect(page.locator('.badge', { hasText: 'scheduled' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Send', exact: true })).toHaveCount(0)

  await page.getByRole('button', { name: 'Cancel schedule' }).click()
  await expect(page.getByText('Schedule cancelled; it is a draft again')).toBeVisible()
  await expect(page.locator('.badge', { hasText: 'draft' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Send', exact: true })).toBeEnabled()
})

test('an admin suppresses an address and removes it again', async ({ page, world, namespace }) => {
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const address = `blocked+${namespace}@e2e.test`

  await page.goto(`/w/${workspace}/p/${project}/mailing/suppressions`)
  await expect(page.getByRole('heading', { name: 'Suppressions' })).toBeVisible()
  await page.getByRole('button', { name: 'Suppress address' }).click()
  await page.getByLabel('Email').fill(address)
  await page.getByLabel('Note').fill('Asked by phone')
  await page.getByRole('button', { name: 'Suppress', exact: true }).click()
  const row = page
    .getByRole('table', { name: 'Suppressions' })
    .getByRole('row', { name: new RegExp(address.replace(/[+.]/g, '\\$&')) })
  await expect(row).toContainText('manual')
  await expect(row).toContainText('Asked by phone')

  await row.getByRole('button', { name: `Remove ${address}` }).click()
  await page.getByRole('button', { name: 'Remove', exact: true }).click()
  await expect(row).toHaveCount(0)
})
