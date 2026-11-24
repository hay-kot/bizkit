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
  const projectPages = [
    'forms',
    'webhooks',
    'extensions',
    'mailing',
    'mailing/broadcasts',
    'mailing/sequences',
    'mailing/suppressions',
  ]
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

  test('an endpoint page with the inspector open is clean', async ({ page, world, baseURL }) => {
    const errors = trackPageErrors(page)
    const workspace = world.workspaces.main!.id
    const created = await page.request.post(`/api/v1/workspaces/${workspace}/webhooks`, {
      data: { name: 'Stripe', project_id: world.projects.site!.id },
      headers: { Origin: new URL(baseURL!).origin },
    })
    expect(created.status()).toBe(201)
    const endpoint = await created.json()
    await page.request.post(endpoint.url, { data: { type: 'charge.succeeded' } })
    await page.goto(`/w/${workspace}/p/${world.projects.site!.id}/webhooks/${endpoint.id}`)
    await expect(page.getByRole('heading', { name: 'Stripe' })).toBeVisible()
    await page.getByRole('button', { name: /^Inspect the POST request/ }).click()
    await expect(page.getByRole('heading', { name: 'Headers' })).toBeVisible()
    await expectCleanPage(page, errors)
  })

  test('a mailing list, a draft broadcast and a sequence are clean', async ({ page, world, baseURL }) => {
    const errors = trackPageErrors(page)
    const workspace = world.workspaces.main!.id
    const headers = { Origin: new URL(baseURL!).origin }
    const created = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/lists`, {
      data: { name: 'Newsletter', project_id: world.projects.site!.id, double_opt_in: false },
      headers,
    })
    expect(created.status()).toBe(201)
    const list = await created.json()
    const contacts = `/api/v1/workspaces/${workspace}/mailing/lists/${list.id}/contacts`
    await page.request.post(contacts, { data: { email: 'ada@e2e.test' }, headers })
    const drafted = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/broadcasts`, {
      data: { list_id: list.id, subject: 'Hello', body_md: 'Hi **there**' },
      headers,
    })
    expect(drafted.status()).toBe(201)
    const broadcast = await drafted.json()
    const project = `/w/${workspace}/p/${world.projects.site!.id}`

    await page.goto(`${project}/mailing/lists/${list.id}`)
    await expect(page.getByRole('row', { name: /ada@e2e\.test/ })).toBeVisible()
    await expectCleanPage(page, errors)

    await page.goto(`${project}/mailing/broadcasts/${broadcast.id}`)
    await expect(page.frameLocator('iframe[title="Mail preview"]').getByText('there')).toBeVisible()
    // The preview is the mail's own HTML in a sandbox without scripts, which
    // axe can't enter; the mail template has its own snapshot tests.
    await expectCleanPage(page, errors, { exclude: ['iframe[title="Mail preview"]'] })

    const sequenced = await page.request.post(`/api/v1/workspaces/${workspace}/mailing/sequences`, {
      data: {
        list_id: list.id,
        name: 'Welcome',
        trigger: 'manual',
        steps: [{ delay_hours: 0, subject: 'Welcome', body_md: 'Hi' }],
      },
      headers,
    })
    expect(sequenced.status()).toBe(201)
    const sequence = await sequenced.json()
    await page.goto(`${project}/mailing/sequences/${sequence.id}`)
    await expect(page.getByRole('group', { name: 'Step 1' })).toBeVisible()
    await expectCleanPage(page, errors)
  })

  const analyticsTabs = ['', '/events', '/funnels', '/people', '/retention', '/settings']
  for (const tab of analyticsTabs) {
    test(`the analytics${tab} page is clean`, async ({ page, world, baseURL }) => {
      const errors = trackPageErrors(page)
      const workspace = world.workspaces.main!.id
      const project = world.projects.site!.id
      const created = await page.request.post(`/api/v1/workspaces/${workspace}/analytics/site`, {
        data: { project_id: project },
        headers: { Origin: new URL(baseURL!).origin },
      })
      expect(created.status()).toBe(201)
      const site = await created.json()
      const beacon = await page.request.post('/m/analytics/public/e', {
        data: { s: site.key, n: 'pageview', u: 'http://site.e2e.test/pricing', w: 1440 },
      })
      expect(beacon.ok()).toBe(true)
      await page.goto(`/w/${workspace}/p/${project}/analytics${tab}`)
      await expect(page.getByTestId('workspace-name')).toHaveText('Team')
      await expect(page.getByRole('navigation', { name: 'Analytics pages' })).toBeVisible()
      await expect(page.locator('.animate-pulse')).toHaveCount(0)
      await expectCleanPage(page, errors)
    })
  }

  test('the open command palette is clean', async ({ page, world }) => {
    const errors = trackPageErrors(page)
    await page.goto(`/w/${world.workspaces.main!.id}/p/${world.projects.site!.id}/forms`)
    await expect(page.getByTestId('workspace-name')).toHaveText('Team')
    await page.getByTestId('open-palette').click()
    await expect(page.getByRole('dialog', { name: 'Command palette' })).toBeVisible()
    await expectCleanPage(page, errors)
  })
})
