import { type APIRequestContext, expect } from '@playwright/test'

export const MAILPIT_URL = process.env.MAILPIT_URL ?? ''

type Summary = { ID: string; To: { Address: string }[] | null }

/**
 * Waits for the newest mail to `email` and returns the path and query of the
 * first link that matches `link`.
 */
export async function emailLinkPath(request: APIRequestContext, email: string, link: RegExp): Promise<string> {
  let id = ''
  await expect
    .poll(
      async () => {
        const resp = await request.get(`${MAILPIT_URL}/api/v1/search?query=${encodeURIComponent(`to:"${email}"`)}`)
        const messages: Summary[] = (await resp.json()).messages ?? []
        id = messages[0]?.ID ?? ''
        return id
      },
      { message: `mail to ${email}`, timeout: 15_000 },
    )
    .not.toBe('')
  const message = await (await request.get(`${MAILPIT_URL}/api/v1/message/${id}`)).json()
  const match = link.exec(`${message.Text}\n${message.HTML ?? ''}`)
  expect(match, `${link} in the mail to ${email}`).not.toBeNull()
  return new URL(match![0], 'http://placeholder').pathname + new URL(match![0], 'http://placeholder').search
}

/** The sign-in link path, `/sign-in/verify?token=...`. */
export function signInLinkPath(request: APIRequestContext, email: string): Promise<string> {
  return emailLinkPath(request, email, /https?:\/\/\S+\/sign-in\/verify\?token=[\w-]+(?:&next=[^\s]+)?/)
}

/** The invite link path, `/invite/<id>`. */
export function inviteLinkPath(request: APIRequestContext, email: string): Promise<string> {
  return emailLinkPath(request, email, /https?:\/\/\S+\/invite\/[0-9a-f-]+/)
}
