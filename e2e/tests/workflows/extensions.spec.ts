import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('scripts filter spam before validation, run after submissions and test-run', async ({
  page,
  request,
  world,
  baseURL,
}) => {
  const headers = { Origin: new URL(baseURL!).origin }
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const api = `/api/v1/workspaces/${workspace}`
  const created = await page.request.post(`${api}/forms`, {
    data: { name: 'Contact', project_id: project },
    headers,
  })
  expect(created.status()).toBe(201)
  const form = await created.json()
  const endpoint = `/m/forms/public/${form.id}`

  await page.goto(`/w/${workspace}/p/${project}/extensions`)
  await page.getByRole('button', { name: 'New script' }).first().click()
  await page.getByLabel('Name').fill('Block casino')
  await page.getByRole('combobox', { name: 'Hook point' }).click()
  await page.getByRole('option', { name: /forms\.pre_validate/ }).click()
  await page.getByRole('button', { name: 'Create script' }).click()
  await expect(page.getByRole('heading', { name: 'Block casino' })).toBeVisible()
  await page
    .getByLabel('Script source')
    .fill(
      "export default (input) => (input.fields.message?.[0]?.includes('casino') ? { action: 'spam', reason: 'casino' } : undefined)",
    )
  await page.getByRole('button', { name: 'Save' }).click()
  await expect(page.getByText('Script saved')).toBeVisible()

  await request.post(endpoint, { data: { name: 'Spammer', message: 'best casino deals' } })
  const spam = await (
    await page.request.get(`${api}/forms/${form.id}/submissions?status=spam`)
  ).json()
  expect(spam.items.map((s: { spam_reason: string }) => s.spam_reason)).toEqual(['casino'])

  await page.goto(`/w/${workspace}/p/${project}/extensions`)
  await page.getByRole('button', { name: 'New script' }).first().click()
  await page.getByLabel('Name').fill('Log submissions')
  await page.getByRole('combobox', { name: 'Hook point' }).click()
  await page.getByRole('option', { name: /forms\.submission_created/ }).click()
  await page.getByRole('button', { name: 'Create script' }).click()
  await expect(page.getByRole('heading', { name: 'Log submissions' })).toBeVisible()
  await page
    .getByLabel('Script source')
    .fill("export default (input, ctx) => { ctx.log('from', input.submission.data.name) }")
  await page.getByRole('button', { name: 'Save' }).click()
  await expect(page.getByText('Script saved')).toBeVisible()

  await page.getByRole('button', { name: 'Run test' }).click()
  await expect(page.getByTestId('test-result')).toContainText('from undefined')

  await request.post(endpoint, { data: { name: 'Ada', message: 'hello' } })
  await page.request.post('/__test/jobs/drain', { headers })
  await expect(async () => {
    await page.reload()
    await expect(page.getByRole('row', { name: /event #1.*from Ada/ })).toBeVisible({
      timeout: 1000,
    })
  }).toPass()
})
