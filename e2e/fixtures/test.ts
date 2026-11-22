import { type Page, test as base } from '@playwright/test'

import { newNamespace, type PersonaName, seedPersona, storageStateFor, type World } from './personas'

type Options = {
  /** The persona to seed for this test; none by default. */
  persona: PersonaName | undefined
  /** The account to sign in as; defaults to `admin`. */
  as: string
}

type Fixtures = {
  /** The seeded rows. Needs `test.use({ persona })`. */
  world: World
  /** A unique namespace for emails and slugs this test makes up. */
  namespace: string
  /** A new signed-in page in its own context, for another seeded account. */
  signIn(key: string): Promise<Page>
}

type Internal = { seeded: World | undefined }

export const test = base.extend<Options & Fixtures & Internal>({
  persona: [undefined, { option: true }],
  as: ['admin', { option: true }],

  namespace: async ({}, use, testInfo) => {
    await use(newNamespace(testInfo.workerIndex, testInfo.retry))
  },

  seeded: async ({ persona, namespace }, use, testInfo) => {
    await use(persona ? await seedPersona(persona, namespace, testInfo.outputPath('manifest.json')) : undefined)
  },

  world: async ({ seeded }, use) => {
    if (!seeded) throw new Error("this test has no persona; call test.use({ persona: 'basic' })")
    await use(seeded)
  },

  storageState: async ({ seeded, as, baseURL }, use) => {
    await use(storageStateFor(baseURL!, seeded?.accounts[as]?.session_token))
  },

  signIn: async ({ browser, world, baseURL }, use) => {
    const contexts: { close(): Promise<void> }[] = []
    await use(async (key) => {
      const context = await browser.newContext({
        baseURL,
        storageState: storageStateFor(baseURL!, world.account(key).session_token),
      })
      contexts.push(context)
      return context.newPage()
    })
    await Promise.all(contexts.map((context) => context.close()))
  },
})

export { expect } from '@playwright/test'
export { signedOut } from './signed-out'
