import { emailLinkPath } from '../../fixtures/mailpit'
import { samplePdf } from '../../fixtures/pdf'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

test('a request goes out, a signer signs through the mailed link, and it seals', async ({
  page,
  request,
  world,
  namespace,
}) => {
  const ada = `ada+${namespace}@e2e.test`
  const project = `/w/${world.workspaces.main!.id}/p/${world.projects.site!.id}`
  await page.goto(`${project}/signing`)
  await page.getByRole('button', { name: 'New request' }).first().click()
  await page.getByLabel('Title').fill('Services agreement')
  await page.getByLabel('PDF').setInputFiles({
    name: 'agreement.pdf',
    mimeType: 'application/pdf',
    buffer: samplePdf('Terms of service'),
  })
  await page.getByLabel('Message').fill('Please sign by Friday.')
  await page.getByLabel('Signer name').fill('Ada Lovelace')
  await page.getByLabel('Signer email').fill(ada)
  await page.getByRole('button', { name: 'Create draft' }).click()
  await expect(page.getByRole('heading', { name: 'Services agreement' })).toBeVisible()
  await expect(page.getByText('draft', { exact: true })).toBeVisible()
  const requestPath = new URL(page.url()).pathname

  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await page.getByRole('button', { name: 'Send', exact: true }).last().click()
  await expect(page.getByText('sent', { exact: true })).toBeVisible()

  const sign = await emailLinkPath(request, ada, /https?:\/\/\S+\/m\/signing\/public\/sign\?t=[\w-]+/)
  await page.goto(sign)
  await expect(page.getByText('Hi Ada Lovelace')).toBeVisible()
  await expect(page.getByLabel('Full name')).toHaveValue('Ada Lovelace')
  await page.getByLabel(/I agree to sign this document electronically/).check()
  await page.getByRole('button', { name: 'Sign' }).click()
  await expect(page.getByRole('heading', { name: 'Thank you' })).toBeVisible()

  // The page polls while the seal runs, so no reload is needed.
  await page.goto(requestPath)
  await expect(page.getByText('completed', { exact: true })).toBeVisible({ timeout: 15_000 })
  const trail = page.getByRole('table', { name: 'Audit trail' })
  await expect(trail).toContainText('Signed: Ada Lovelace')
  await expect(trail).toContainText('Sealed')

  const download = page.waitForEvent('download')
  await page.getByRole('button', { name: 'Signed PDF' }).click()
  expect((await download).suggestedFilename()).toBe('agreement-signed.pdf')
})
