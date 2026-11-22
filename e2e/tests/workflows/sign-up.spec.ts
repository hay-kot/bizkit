import { signInLinkPath } from '../../fixtures/mailpit'
import { expect, test } from '../../fixtures/test'

test('a new email signs up, creates a workspace and is its admin', async ({ page, request, namespace, baseURL }) => {
  const email = `new+${namespace}@signup.e2e.test`
  await page.goto('/sign-in')
  await page.getByLabel('Email').fill(email)
  await page.getByRole('button', { name: 'Email me a link' }).click()
  await expect(page.getByTestId('link-sent')).toContainText(email)

  await page.goto(await signInLinkPath(request, email))
  await expect(page).toHaveURL(`${baseURL}/`)
  await expect(page.getByTestId('signed-in-as')).toHaveText(email)

  await page.getByRole('button', { name: 'New workspace' }).click()
  await page.getByLabel('Name').fill(`Acme ${namespace}`)
  await expect(page.getByLabel('Slug')).toHaveValue(`acme-${namespace}`)
  await page.getByRole('button', { name: 'Create workspace' }).click()
  await expect(page).toHaveURL(/\/w\/[0-9a-f-]+\/projects$/)
  await expect(page.getByTestId('workspace-name')).toHaveText(`Acme ${namespace}`)

  await page.getByRole('link', { name: 'Team' }).click()
  await expect(page.getByRole('combobox', { name: `Role of ${email}` })).toHaveAttribute('data-value', 'admin')
})
