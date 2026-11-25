import type { APIRequestContext, Locator, Page } from '@playwright/test'

import { emailLinkPath, MAILPIT_URL } from '../../fixtures/mailpit'
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

/** How many mails Mailpit holds for `email`. */
async function mailCount(request: APIRequestContext, email: string): Promise<number> {
  const resp = await request.get(`${MAILPIT_URL}/api/v1/search?query=${encodeURIComponent(`to:"${email}"`)}`)
  return ((await resp.json()).messages ?? []).length
}

/** Clicks the page overlay at a point given in page fractions. */
async function clickPage(overlay: Locator, fx: number, fy: number) {
  const box = (await overlay.boundingBox())!
  await overlay.click({ position: { x: box.width * fx, y: box.height * fy } })
}

/** Signs from the mailed link in a context of its own, with a typed signature. */
async function signAs(
  page: Page,
  link: string,
  signer: { name: string },
  fill: (page: Page) => Promise<void>,
) {
  await page.goto(link)
  await expect(page.getByRole('heading', { name: 'Services agreement' })).toBeVisible()
  await expect(page.getByText(`For ${signer.name}`)).toBeVisible()
  await page.getByRole('button', { name: 'Adopt signature' }).click()
  const dialog = page.getByRole('dialog', { name: 'Adopt your signature' })
  await dialog.getByRole('radio', { name: 'Type' }).click()
  await dialog.getByRole('button', { name: 'Adopt' }).click()
  await fill(page)
  await page.getByRole('checkbox', { name: /I agree to sign this document electronically/ }).check()
  await page.getByRole('button', { name: 'Sign', exact: true }).click()
  await expect(page.getByRole('heading', { name: 'Thank you, you signed' })).toBeVisible()
}

test.describe('the sender', () => {
  // Tall enough that a whole letter page fits, so clicks on the page land.
  test.use({ viewport: { width: 1280, height: 1800 } })

  test('uploads a PDF, places fields for two signing-order groups, sends, and gets the sealed PDF', async ({
    page,
    browser,
    request,
    world,
    namespace,
    baseURL,
  }) => {
    const ada = { name: 'Ada Lovelace', email: `ada+${namespace}@e2e.test` }
    const bo = { name: 'Bo Ramsey', email: `bo+${namespace}@e2e.test` }
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
    await page.getByLabel('Signer name').fill(ada.name)
    await page.getByLabel('Signer email').fill(ada.email)
    await page.getByRole('button', { name: 'Add signer' }).click()
    await page.getByLabel('Signer name').nth(1).fill(bo.name)
    await page.getByLabel('Signer email').nth(1).fill(bo.email)
    await page.getByLabel('Signing order').nth(1).fill('2')
    await page.getByRole('button', { name: 'Create draft' }).click()

    await expect(page.getByRole('heading', { name: 'Services agreement' })).toBeVisible()
    await expect(page.getByText('draft', { exact: true })).toBeVisible()
    const requestPath = new URL(page.url()).pathname
    await expect(page.getByTestId('unplaced')).toContainText('Ada Lovelace, Bo Ramsey')

    const overlay = page.locator('[data-page="1"]')
    await expect(page.getByRole('region', { name: /agreement\.pdf, page 1 of 1/ })).toBeVisible()
    const kinds = page.getByRole('group', { name: 'Field kinds' })
    const settings = page.getByTestId('field-settings')

    // Ada, order 1: a signature, initials, the date and a text field.
    await kinds.getByRole('button', { name: 'Signature', exact: true }).click()
    await clickPage(overlay, 0.3, 0.55)
    await expect(page.getByRole('button', { name: 'Signature for Ada Lovelace, page 1' })).toBeFocused()
    await kinds.getByRole('button', { name: 'Initials', exact: true }).click()
    await clickPage(overlay, 0.75, 0.55)
    await kinds.getByRole('button', { name: 'Date signed', exact: true }).click()
    await clickPage(overlay, 0.3, 0.62)
    await kinds.getByRole('button', { name: 'Text', exact: true }).click()
    await clickPage(overlay, 0.3, 0.68)
    await settings.getByLabel('Label').fill('Company')

    // Bo, order 2: a signature moved by keyboard, a dragged-in checkbox and the date.
    await page.getByRole('combobox', { name: 'Signer for new fields' }).click()
    await page.getByRole('option', { name: bo.name }).click()
    await kinds.getByRole('button', { name: 'Signature', exact: true }).click()
    await clickPage(overlay, 0.3, 0.8)
    const boSignature = page.getByRole('button', { name: 'Signature for Bo Ramsey, page 1' })
    const before = (await boSignature.boundingBox())!
    await boSignature.press('Shift+ArrowRight')
    await expect.poll(async () => (await boSignature.boundingBox())!.x).toBeGreaterThan(before.x)
    const box = (await overlay.boundingBox())!
    await kinds
      .getByRole('button', { name: 'Checkbox', exact: true })
      .dragTo(overlay, { targetPosition: { x: box.width * 0.8, y: box.height * 0.8 } })
    await expect(page.getByRole('button', { name: 'Checkbox for Bo Ramsey, page 1' })).toBeVisible()
    await settings.getByLabel('Label').fill('Terms')
    await kinds.getByRole('button', { name: 'Date signed', exact: true }).click()
    await clickPage(overlay, 0.3, 0.88)
    await expect(page.getByTestId('unplaced')).toHaveCount(0)

    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Saved' })).toBeVisible()
    await page.reload()
    await expect(page.getByRole('button', { name: 'Text "Company" for Ada Lovelace, page 1' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Checkbox "Terms" for Bo Ramsey, page 1' })).toBeVisible()

    await page.getByRole('button', { name: 'Send', exact: true }).click()
    await page.getByRole('dialog').getByRole('button', { name: 'Send', exact: true }).click()
    await expect(page.getByText('sent', { exact: true })).toBeVisible()

    const adaLink = await emailLinkPath(request, ada.email, /https?:\/\/\S+\/sign\?t=[\w-]+/)
    expect(await mailCount(request, bo.email), 'order 2 waits for order 1').toBe(0)

    const adaContext = await browser.newContext({ baseURL })
    const boContext = await browser.newContext({ baseURL })
    try {
      await signAs(await adaContext.newPage(), adaLink, ada, async (signer) => {
        await signer.getByRole('button', { name: 'Adopt initials' }).click()
        const initials = signer.getByRole('dialog', { name: 'Adopt your initials' })
        await initials.getByRole('radio', { name: 'Type' }).click()
        await initials.getByRole('button', { name: 'Adopt' }).click()
        await signer.getByLabel('Page 1, Text "Company"').fill('Analytical Engines Ltd')
      })

      const boLink = await emailLinkPath(request, bo.email, /https?:\/\/\S+\/sign\?t=[\w-]+/)
      await signAs(await boContext.newPage(), boLink, bo, async (signer) => {
        await signer.getByRole('checkbox', { name: 'Page 1, Checkbox "Terms"' }).check()
      })
    } finally {
      await adaContext.close()
      await boContext.close()
    }

    // The page polls while the seal runs, so no reload is needed.
    await page.goto(requestPath)
    await expect(page.getByText('completed', { exact: true })).toBeVisible({ timeout: 20_000 })
    const trail = page.getByRole('list', { name: 'Audit trail' })
    await expect(trail).toContainText('Signed: Ada Lovelace')
    await expect(trail).toContainText('Signed: Bo Ramsey')
    await expect(trail).toContainText('Sealed')
    await expect(page.getByRole('img', { name: /Text "Company" for Ada Lovelace .*filled: Analytical Engines Ltd/ })).toBeVisible()

    const download = page.waitForEvent('download')
    await page.getByRole('link', { name: 'Signed PDF' }).click()
    const file = await download
    expect(file.suggestedFilename()).toBe('agreement-signed.pdf')
    const signedHref = await page.getByRole('link', { name: 'Signed PDF' }).getAttribute('href')
    const bytes = await (await page.request.get(signedHref!)).body()
    expect(bytes.subarray(0, 5).toString('latin1')).toBe('%PDF-')
    expect(bytes.includes(Buffer.from('ETSI.CAdES.detached'))).toBe(true)
  })
})
