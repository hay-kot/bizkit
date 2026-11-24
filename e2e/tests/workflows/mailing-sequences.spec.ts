import { emailLinkPath } from '../../fixtures/mailpit'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('a new subscriber gets the first step of a sequence and can be taken out', async ({
  page,
  request,
  world,
  namespace,
  baseURL,
}) => {
  const workspace = world.workspaces.main!.id
  const project = `/w/${workspace}/p/${world.projects.site!.id}`
  const headers = { Origin: new URL(baseURL!).origin }
  const created = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/lists`, {
    data: { name: 'Product news', project_id: world.projects.site!.id, double_opt_in: false },
    headers,
  })
  expect(created.status()).toBe(201)
  const list = await created.json()

  await page.goto(`${project}/mailing/sequences`)
  await page.getByRole('button', { name: 'New sequence' }).first().click()
  await page.getByLabel('Name').fill('Welcome series')
  await page.getByRole('button', { name: 'Create sequence' }).click()
  await expect(page.getByRole('heading', { name: 'Welcome series' })).toBeVisible()

  await page.getByRole('button', { name: 'Add step' }).click()
  const first = page.getByRole('group', { name: 'Step 1' })
  await first.getByLabel('Subject').fill(`Welcome ${namespace}`)
  await first.getByLabel('Body').fill('Hi {{ name }}, thanks for joining.')
  await page.getByRole('button', { name: 'Add step' }).click()
  const second = page.getByRole('group', { name: 'Step 2' })
  await expect(second.getByLabel('Wait (hours)')).toHaveValue('24')
  await second.getByLabel('Subject').fill('Day one')
  await page.getByText('Enroll new contacts').click()
  await page.getByRole('button', { name: 'Save' }).click()
  await expect(page.getByRole('button', { name: 'Saved' })).toBeVisible()

  const lea = `lea+${namespace}@e2e.test`
  const added = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/lists/${list.id}/contacts`, {
    data: { email: lea, name: 'Lea' },
    headers,
  })
  expect(added.status()).toBe(201)
  await emailLinkPath(request, lea, /https?:\/\/\S+\/m\/mailing\/public\/unsubscribe\?t=[\w-]+/)

  await page.reload()
  const enrollments = page.getByRole('table', { name: 'Enrollments' })
  const row = enrollments.getByRole('row', { name: new RegExp(lea.replace(/[+.]/g, '\\$&')) })
  await expect(row).toContainText('active')
  await expect(row).toContainText('1 of 2')
  await expect(page.getByRole('table', { name: 'Send log' }).getByRole('row', { name: /sent/ })).toBeVisible({
    timeout: 15_000,
  })

  await row.getByRole('button', { name: `Take ${lea} out` }).click()
  await page.getByRole('button', { name: 'Take out' }).click()
  await expect(row).toContainText('exited')
})
