import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('build an insight with the builder, save it, then switch to Code and back', async ({
  page,
  playwright,
  world,
  baseURL,
}) => {
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const base = `/api/v1/workspaces/${workspace}/analytics`
  const api = await playwright.request.newContext({
    baseURL,
    extraHTTPHeaders: { authorization: `Bearer ${world.api_keys.full!.token}` },
  })
  try {
    expect((await api.post(`${base}/site`, { data: { project_id: project } })).status()).toBe(201)
    const signup = (user_id: string, plan: string, source: string) => ({
      name: 'signup',
      user_id,
      props: { plan, source },
    })
    const events = [
      signup('ada', 'pro', 'ads'),
      signup('bob', 'pro', 'blog'),
      signup('cy', 'pro', 'ads'),
      signup('dee', 'free', 'ads'),
    ]
    expect((await api.post(`${base}/events`, { data: { project_id: project, events } })).status()).toBe(200)
    const hasEvents = async () => {
      const site = (await (await api.get(`${base}/site?project_id=${project}`)).json()) as { has_events: boolean }
      return site.has_events
    }
    await expect.poll(hasEvents).toBe(true)

    await page.goto(`/w/${workspace}/p/${project}/analytics/insights/new`)
    const builder = page.getByRole('region', { name: 'Query builder' })
    const result = page.getByRole('region', { name: 'Result' })

    // The event list comes from the schema, with counts.
    await builder.getByRole('combobox', { name: 'Event' }).click()
    await page.getByRole('option', { name: /signup/ }).click()
    await expect(result).toContainText('4')

    // A filter from the key and value suggestions.
    await builder.getByRole('button', { name: 'Add filter' }).click()
    const popover = page.getByRole('dialog', { name: 'Add filter' })
    await expect(popover.getByRole('combobox', { name: 'Property' })).toBeFocused()
    await popover.getByRole('combobox', { name: 'Property' }).click()
    await page.getByRole('option', { name: /plan/ }).click()
    await popover.getByRole('combobox', { name: 'Value' }).click()
    await expect(page.getByRole('option', { name: /pro/ })).toContainText('3')
    await page.getByRole('option', { name: /pro/ }).click()
    await popover.getByRole('button', { name: 'Add filter' }).click()
    await expect(popover).toBeHidden()
    await expect(builder.getByRole('button', { name: 'Edit filter plan is pro' })).toBeVisible()

    await builder.getByRole('combobox', { name: 'Breakdown' }).click()
    await page.getByRole('option', { name: /source/ }).click()
    await expect(result.getByRole('listitem').filter({ hasText: 'ads' })).toContainText('2')

    await page.getByRole('button', { name: 'Save insight' }).click()
    const save = page.getByRole('dialog', { name: 'Save insight' })
    await save.getByLabel('Name').fill('Pro signups by source')
    await save.getByRole('button', { name: 'Save insight' }).click()
    await expect(page.getByRole('heading', { name: 'Pro signups by source' })).toBeVisible()

    const insights = (await (await api.get(`${base}/insights?project_id=${project}`)).json()) as {
      items: { name: string; query: unknown }[]
    }
    expect(insights.items.find((i) => i.name === 'Pro signups by source')?.query).toEqual({
      kind: 'events',
      event: 'signup',
      measure: 'total',
      filters: [{ key: 'plan', op: 'is', value: 'pro' }],
      breakdown: { source: 'property', key: 'source' },
    })

    // Code shows the same query as JSON, and the builder reads it back.
    const modes = page.getByRole('radiogroup', { name: 'Query editor' })
    await modes.getByRole('radio', { name: 'Code' }).click()
    const code = page.getByRole('region', { name: 'Query JSON' })
    await expect(code).toContainText('"key": "plan"')
    await expect(builder).toBeHidden()
    await modes.getByRole('radio', { name: 'Builder' }).click()
    await expect(builder.getByRole('button', { name: 'Edit filter plan is pro' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Saved' })).toBeDisabled()

    // JSON the builder can't show keeps the editor on Code; Cmd/Ctrl+Enter runs it.
    await modes.getByRole('radio', { name: 'Code' }).click()
    const editor = code.getByRole('textbox')
    await editor.click()
    await page.keyboard.press('ControlOrMeta+a')
    await page.keyboard.insertText('{"kind": "events", "event": "signup", "measure": "users", "colour": "red"}')
    await modes.getByRole('radio', { name: 'Builder' }).click()
    await expect(page.getByRole('alert').filter({ hasText: 'The builder has no field "colour".' })).toBeVisible()
    await expect(modes.getByRole('radio', { name: 'Code' })).toHaveAttribute('aria-checked', 'true')

    await editor.click()
    await page.keyboard.press('ControlOrMeta+a')
    await page.keyboard.insertText('{"kind": "events", "event": "signup", "measure": "users"}')
    await editor.press('ControlOrMeta+Enter')
    await expect(result.getByRole('img', { name: /^Users from/ })).toBeVisible()
    await modes.getByRole('radio', { name: 'Builder' }).click()
    await expect(builder.getByRole('combobox', { name: 'Measure' })).toHaveText(/Unique users/)
    await expect(builder.getByRole('button', { name: /Edit filter/ })).toHaveCount(0)
  } finally {
    await api.dispose()
  }
})
