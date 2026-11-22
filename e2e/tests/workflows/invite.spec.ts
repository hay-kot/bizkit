import { inviteLinkPath, signInLinkPath } from '../../fixtures/mailpit'
import { signedOut } from '../../fixtures/signed-out'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('an admin invites a new email, who signs in from the mail and joins', async ({
  page,
  browser,
  baseURL,
  request,
  world,
  namespace,
}) => {
  const invitee = `guest+${namespace}@outside.e2e.test`
  const workspaceId = world.workspaces.main!.id
  await page.goto(`/w/${workspaceId}/team`)
  await page.getByRole('button', { name: 'Invite', exact: true }).click()
  await page.getByLabel('Email').fill(invitee)
  await page.getByRole('button', { name: 'Send invitation' }).click()
  await expect(page.getByText(`Invitation sent to ${invitee}`)).toBeVisible()
  await expect(page.getByRole('cell', { name: invitee, exact: true })).toBeVisible()

  const invitePath = await inviteLinkPath(request, invitee)
  const context = await browser.newContext({ baseURL, storageState: signedOut })
  const guest = await context.newPage()
  await guest.goto(invitePath)
  await expect(guest).toHaveURL(/\/sign-in\?next=/)
  await guest.getByLabel('Email').fill(invitee)
  await guest.getByRole('button', { name: 'Email me a link' }).click()
  await expect(guest.getByTestId('link-sent')).toBeVisible()

  await guest.goto(await signInLinkPath(request, invitee))
  await expect(guest).toHaveURL(new RegExp(`${invitePath}$`))
  await expect(guest.getByRole('heading', { name: 'Join Team' })).toBeVisible()
  await guest.getByRole('button', { name: 'Accept invitation' }).click()
  await expect(guest).toHaveURL(
    new RegExp(`/w/${workspaceId}/p/${world.projects.app!.id}/forms$`),
  )
  await context.close()

  await page.reload()
  const members = page.getByRole('region', { name: 'Members' })
  await expect(members.getByRole('cell', { name: invitee, exact: true })).toBeVisible()
  await expect(
    page.getByRole('region', { name: 'Pending invitations' }).getByText(invitee),
  ).toHaveCount(0)
})
