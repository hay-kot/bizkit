import { emailLinkPath } from '../../fixtures/mailpit'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('a reader leaves one list from the manage page, then an admin erases the address', async ({
  page,
  request,
  world,
  baseURL,
  namespace,
}) => {
  const workspace = world.workspaces.main!.id
  const project = world.projects.site!.id
  const api = `/api/v1/workspaces/${workspace}/mailing`
  const headers = { Origin: new URL(baseURL!).origin }
  const mo = `mo+${namespace}@e2e.test`
  const moRow = new RegExp(mo.replace(/[+.]/g, '\\$&'))

  const lists: { id: string; name: string }[] = []
  for (const name of [`News ${namespace}`, `Offers ${namespace}`]) {
    const created = await page.request.post(`${api}/lists`, {
      data: { name, project_id: project, double_opt_in: false },
      headers,
    })
    const list = await created.json()
    lists.push(list)
    const added = await page.request.post(`${api}/lists/${list.id}/contacts`, { data: { email: mo }, headers })
    expect(added.status()).toBe(201)
  }
  const [news, offers] = lists as [{ id: string; name: string }, { id: string; name: string }]
  const drafted = await page.request.post(`${api}/broadcasts`, {
    data: { list_id: news.id, subject: `Privacy ${namespace}`, body_md: 'Hello' },
    headers,
  })
  const broadcast = await drafted.json()
  const sent = await page.request.post(`${api}/broadcasts/${broadcast.id}/send`, { data: {}, headers })
  expect(sent.status()).toBe(200)

  // Opening the link changes nothing; Save unsubscribes the unticked list.
  const manage = await emailLinkPath(request, mo, /https?:\/\/\S+\/m\/mailing\/public\/manage\?t=[\w.-]+/)
  await page.goto(manage)
  await expect(page.getByRole('heading', { name: 'Your subscriptions' })).toBeVisible()
  await page.getByLabel(offers.name).uncheck()
  await page.getByRole('button', { name: 'Save' }).click()
  await expect(page.getByText('Saved.')).toBeVisible()
  await expect(page.getByText(`${offers.name}: unsubscribed`)).toBeVisible()
  await expect(page.getByLabel(news.name)).toBeChecked()

  await page.goto(`/w/${workspace}/p/${project}/mailing/lists/${offers.id}`)
  await expect(page.getByRole('row', { name: moRow })).toContainText('unsubscribed')
  await page.goto(`/w/${workspace}/p/${project}/mailing/lists/${news.id}`)
  await expect(page.getByRole('row', { name: moRow })).toContainText('subscribed')

  await page.goto(`/w/${workspace}/p/${project}/mailing/suppressions`)
  await page.getByRole('button', { name: 'Erase address' }).click()
  const dialog = page.getByRole('dialog', { name: 'Erase address' })
  await dialog.getByLabel('Email').fill(mo)
  await dialog.getByRole('button', { name: 'Erase', exact: true }).click()
  await page.getByRole('dialog', { name: `Erase ${mo}?` }).getByRole('button', { name: 'Erase', exact: true }).click()
  await expect(page.getByText(`Erased ${mo}: 2 contacts`)).toBeVisible()

  await page.goto(`/w/${workspace}/p/${project}/mailing/lists/${news.id}`)
  await expect(page.getByText('No contacts')).toBeVisible()
  const again = await page.request.post(`${api}/lists/${news.id}/contacts`, { data: { email: mo }, headers })
  expect(again.status()).toBe(409)
})
