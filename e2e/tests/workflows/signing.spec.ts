import type { APIRequestContext, Page } from '@playwright/test'

import { emailLinkPath } from '../../fixtures/mailpit'
import { samplePdf } from '../../fixtures/pdf'
import { expect, test } from '../../fixtures/test'

test.use({ persona: 'team' })

type Field = { kind: string; x: number; y: number; width: number; height: number; label?: string; required?: boolean }

/**
 * Creates a request for one signer through the API, places `fields` on page
 * 1, sends it and returns the signer view's path from the mailed link.
 */
async function sendRequest(
  api: APIRequestContext,
  request: APIRequestContext,
  workspace: string,
  project: string,
  signer: { name: string; email: string },
  fields: Field[],
): Promise<string> {
  const base = `/api/v1/workspaces/${workspace}/signing/requests`
  const created = await api.post(base, {
    data: {
      project_id: project,
      title: 'Services agreement',
      message: 'Please sign by Friday.',
      file_name: 'agreement.pdf',
      pdf_base64: samplePdf('Terms of service').toString('base64'),
      signers: [signer],
    },
  })
  expect(created.status(), await created.text()).toBe(201)
  const draft = (await created.json()) as { id: string; signers: { id: string }[] }
  const signerId = draft.signers[0]!.id
  const placed = await api.put(`${base}/${draft.id}/fields`, {
    data: { fields: fields.map((field) => ({ signer_id: signerId, page: 1, ...field })) },
  })
  expect(placed.status(), await placed.text()).toBe(200)
  const sent = await api.post(`${base}/${draft.id}/send`)
  expect(sent.status(), await sent.text()).toBe(200)
  return emailLinkPath(request, signer.email, /https?:\/\/\S+\/sign\?t=[\w-]+/)
}

/** Waits for the seal, then downloads the sealed PDF from the done view. */
async function expectSealedPdf(page: Page) {
  await expect(page.getByRole('heading', { name: 'Thank you, you signed' })).toBeVisible()
  const link = page.getByRole('link', { name: 'Download the signed PDF' })
  await expect(link).toBeVisible({ timeout: 20_000 })
  const download = page.waitForEvent('download')
  await link.click()
  const file = await download
  expect(file.suggestedFilename()).toBe('agreement-signed.pdf')
  const bytes = await (await page.request.get((await link.getAttribute('href'))!)).body()
  expect(bytes.subarray(0, 5).toString('latin1')).toBe('%PDF-')
  expect(bytes.includes(Buffer.from('ETSI.CAdES.detached'))).toBe(true)
}

test('a signer draws a signature, fills their fields and downloads the sealed PDF', async ({
  page,
  playwright,
  request,
  world,
  namespace,
  baseURL,
}) => {
  const api = await playwright.request.newContext({
    baseURL,
    extraHTTPHeaders: { authorization: `Bearer ${world.api_keys.full!.token}` },
  })
  const ada = { name: 'Ada Lovelace', email: `ada+${namespace}@e2e.test` }
  try {
    const sign = await sendRequest(api, request, world.workspaces.main!.id, world.projects.site!.id, ada, [
      { kind: 'signature', x: 0.1, y: 0.6, width: 0.35, height: 0.08 },
      { kind: 'initials', x: 0.6, y: 0.6, width: 0.12, height: 0.06 },
      { kind: 'date', x: 0.1, y: 0.7, width: 0.2, height: 0.04 },
      { kind: 'text', x: 0.1, y: 0.76, width: 0.4, height: 0.04, label: 'Company' },
      { kind: 'checkbox', x: 0.1, y: 0.82, width: 0.04, height: 0.03, label: 'Terms' },
    ])
    await page.goto(sign)
    await expect(page.getByRole('heading', { name: 'Services agreement' })).toBeVisible()
    await expect(page.getByTestId('request-message')).toHaveText('Please sign by Friday.')
    await expect(page.getByRole('region', { name: /agreement\.pdf, page 1 of 1/ })).toBeVisible()
    const signButton = page.getByRole('button', { name: 'Sign', exact: true })
    await expect(signButton).toBeDisabled()

    await page.getByRole('button', { name: 'Page 1, Signature: add your signature' }).click()
    const dialog = page.getByRole('dialog', { name: 'Adopt your signature' })
    const pad = dialog.getByTestId('signature-pad')
    const box = (await pad.boundingBox())!
    await page.mouse.move(box.x + box.width * 0.1, box.y + box.height * 0.7)
    await page.mouse.down()
    for (const [fx, fy] of [[0.3, 0.2], [0.5, 0.8], [0.7, 0.3], [0.9, 0.6]] as const) {
      await page.mouse.move(box.x + box.width * fx, box.y + box.height * fy, { steps: 5 })
    }
    await page.mouse.up()
    await dialog.getByRole('button', { name: 'Adopt' }).click()
    await expect(page.getByRole('button', { name: 'Page 1, Signature: change your signature' })).toBeVisible()

    await page.getByRole('button', { name: 'Page 1, Initials: add your initials' }).click()
    const initials = page.getByRole('dialog', { name: 'Adopt your initials' })
    await initials.getByRole('radio', { name: 'Type' }).click()
    await expect(initials.getByLabel('Type your initials')).toHaveValue('AL')
    await initials.getByRole('button', { name: 'Adopt' }).click()
    await expect(page.getByTestId('adopted-initials')).toHaveText('AL')

    await page.getByLabel('Page 1, Text "Company"').fill('Analytical Engines Ltd')
    await page.getByRole('checkbox', { name: 'Page 1, Checkbox "Terms"' }).check()
    await expect(page.getByRole('note', { name: /Page 1, Date: set to \d{4}-\d{2}-\d{2}/ })).toBeVisible()
    await expect(signButton).toBeDisabled()
    await page.getByRole('checkbox', { name: /I agree to sign this document electronically/ }).check()
    await signButton.click()
    await expectSealedPdf(page)
  } finally {
    await api.dispose()
  }
})

test('a signer types a signature and signs', async ({ page, playwright, request, world, namespace, baseURL }) => {
  const api = await playwright.request.newContext({
    baseURL,
    extraHTTPHeaders: { authorization: `Bearer ${world.api_keys.full!.token}` },
  })
  const bo = { name: 'Bo Ramsey', email: `bo+${namespace}@e2e.test` }
  try {
    const sign = await sendRequest(api, request, world.workspaces.main!.id, world.projects.site!.id, bo, [
      { kind: 'signature', x: 0.5, y: 0.85, width: 0.3, height: 0.08 },
    ])
    await page.goto(sign)
    await page.getByRole('button', { name: 'Adopt signature' }).click()
    const dialog = page.getByRole('dialog', { name: 'Adopt your signature' })
    await dialog.getByRole('radio', { name: 'Type' }).click()
    await expect(dialog.getByLabel('Type your signature')).toHaveValue('Bo Ramsey')
    await dialog.getByRole('button', { name: 'Adopt' }).click()
    await expect(page.getByTestId('adopted-signature')).toHaveText('Bo Ramsey')
    await page.getByRole('checkbox', { name: /I agree to sign this document electronically/ }).check()
    await page.getByRole('button', { name: 'Sign', exact: true }).click()
    await expectSealedPdf(page)

    // The link stays good after signing: it opens the done view with the download.
    await page.reload()
    await expect(page.getByRole('link', { name: 'Download the signed PDF' })).toBeVisible()
  } finally {
    await api.dispose()
  }
})

test('a link that no longer works says so', async ({ page }) => {
  await page.goto('/sign?t=not-a-real-token')
  await expect(page.getByRole('heading', { name: 'This link no longer works' })).toBeVisible()
})
