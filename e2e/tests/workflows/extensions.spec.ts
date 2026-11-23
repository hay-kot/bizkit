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

  const broken = 'export default (input, ctx) => {'
  await page.getByLabel('Script source').fill(broken)
  const refetched = page.waitForResponse(
    (r) => r.request().method() === 'GET' && /\/extensions\/scripts\/[^/?]+$/.test(r.url()),
  )
  await page.getByRole('button', { name: 'Save' }).click()
  await expect(page.getByText(/unexpected token/).first()).toBeVisible()
  await refetched
  await expect(page.getByLabel('Script source')).toHaveText(broken)
  await page
    .getByLabel('Script source')
    .fill("export default (input, ctx) => { ctx.log('from', input.submission.data.name) }")

  await page.getByLabel('Test cases').fill(
    JSON.stringify([
      { name: 'Logs the name', input: { submission: { data: { name: 'Ada' } } } },
      {
        name: 'Returns nothing',
        input: { submission: { data: { name: 'Bo' } } },
        expect: { ok: true },
      },
    ]),
  )
  await page.getByRole('button', { name: 'Run all' }).click()
  await expect(page.getByRole('status').filter({ hasText: '1 of 2 passed' })).toBeVisible()
  const cases = page.getByRole('table', { name: 'Case results' })
  await expect(cases.getByRole('row', { name: /Logs the name.*Pass/ })).toBeVisible()
  await cases.getByRole('button', { name: 'Returns nothing', exact: true }).click()
  await expect(page.getByTestId('case-diff')).toContainText('+ null')
  await page.keyboard.press('Escape')
  await page.getByRole('button', { name: 'Save' }).click()
  await expect(page.getByText('Script saved')).toBeVisible()

  await request.post(endpoint, { data: { name: 'Ada', message: 'hello' } })
  await page.request.post('/__test/jobs/drain', { headers })
  await expect(async () => {
    await page.reload()
    await expect(page.getByRole('row', { name: /event #1.*from Ada/ })).toBeVisible({
      timeout: 1000,
    })
  }).toPass()
})

test('one script runs for the forms it names, another for all forms', async ({
  page,
  request,
  world,
  baseURL,
}) => {
  const headers = { Origin: new URL(baseURL!).origin }
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const api = `/api/v1/workspaces/${workspace}`
  const forms: Record<string, string> = {}
  for (const name of ['Contact', 'Quote', 'Careers']) {
    const created = await page.request.post(`${api}/forms`, {
      data: { name, project_id: project },
      headers,
    })
    expect(created.status()).toBe(201)
    forms[name] = (await created.json()).id
  }
  const formPage = (name: string) => `/w/${workspace}/p/${project}/forms/${forms[name]}`
  const section = page.getByRole('region', { name: 'Extensions' })

  await page.goto(formPage('Contact'))
  await section.getByRole('button', { name: 'New script' }).click()
  await page.getByLabel('Name').fill('Shared casino filter')
  await page.getByRole('combobox', { name: 'Hook point' }).click()
  await page.getByRole('option', { name: /forms\.pre_validate/ }).click()
  await expect(page.getByRole('checkbox', { name: 'Contact' })).toBeChecked()
  await page.getByRole('checkbox', { name: 'Careers' }).check()
  await page.getByRole('button', { name: 'Create script' }).click()
  await expect(page.getByRole('heading', { name: 'Shared casino filter' })).toBeVisible()
  await page
    .getByLabel('Script source')
    .fill(
      "export default (input) => (input.fields.message?.[0]?.includes('casino') ? { action: 'spam', reason: 'shared' } : undefined)",
    )
  await page.getByRole('button', { name: 'Save' }).click()
  await expect(page.getByText('Script saved')).toBeVisible()

  await page.goto(`/w/${workspace}/p/${project}/extensions`)
  await page.getByRole('button', { name: 'New script' }).first().click()
  await page.getByLabel('Name').fill('Notify everyone')
  await page.getByRole('combobox', { name: 'Hook point' }).click()
  await page.getByRole('option', { name: /forms\.submission_created/ }).click()
  await expect(page.getByRole('checkbox', { name: 'All forms' })).toBeChecked()
  await page.getByRole('button', { name: 'Create script' }).click()
  await expect(page.getByRole('heading', { name: 'Notify everyone' })).toBeVisible()

  await page.goto(`/w/${workspace}/p/${project}/extensions`)
  const scripts = page.getByRole('table', { name: 'Scripts' })
  await expect(scripts.getByRole('row', { name: /Shared casino filter.*Contact, Careers/ })).toBeVisible()
  await expect(scripts.getByRole('row', { name: /Notify everyone.*All forms/ })).toBeVisible()

  await page.goto(formPage('Quote'))
  await expect(section.getByRole('row', { name: /Notify everyone.*All forms/ })).toBeVisible()
  await expect(section.getByRole('row', { name: /Shared casino filter/ })).toHaveCount(0)
  await page.goto(formPage('Careers'))
  await expect(section.getByRole('row', { name: /Shared casino filter/ })).toBeVisible()

  for (const name of ['Quote', 'Careers']) {
    await request.post(`/m/forms/public/${forms[name]}`, {
      data: { name: 'Spammer', message: 'best casino deals' },
    })
  }
  const statusOf = async (name: string) =>
    (await (await page.request.get(`${api}/forms/${forms[name]}/submissions?status=spam`)).json())
      .items.length
  expect(await statusOf('Careers')).toBe(1)
  expect(await statusOf('Quote')).toBe(0)
})
