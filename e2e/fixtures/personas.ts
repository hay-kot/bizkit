import { execFile } from 'node:child_process'
import { readFile } from 'node:fs/promises'
import { promisify } from 'node:util'

const run = promisify(execFile)

export type PersonaName = 'basic' | 'team'

export type Account = { id: string; email: string; session_token: string }

/** What `bizkit dev seed-persona` wrote, keyed the way the persona names things. */
export type Manifest = {
  persona: PersonaName
  namespace: string
  accounts: Record<string, Account>
  workspaces: Record<string, { id: string; slug: string }>
  projects: Record<string, { id: string; slug: string }>
  api_keys: Record<string, { id: string; token: string }>
  invitations: Record<string, { id: string; email: string }>
}

export type World = Manifest & { account(key: string): Account }

/** A namespace per worker and retry, so a retry never meets a half-seeded one. */
export function newNamespace(workerIndex: number, retry: number): string {
  const random = Math.random().toString(36).replace(/[^a-z0-9]/g, '').padEnd(6, '0').slice(0, 6)
  return `w${workerIndex}r${retry}${random}`
}

function seedCommand(): string[] {
  const cmd = process.env.E2E_SEED_CMD
  if (!cmd) {
    throw new Error('E2E_SEED_CMD is not set; run the suite through `mise run test:e2e`')
  }
  return cmd.split(' ').filter(Boolean)
}

/** Seeds `persona` under `namespace` by running the seed command. */
export async function seedPersona(persona: PersonaName, namespace: string, outFile: string): Promise<World> {
  const [bin, ...args] = seedCommand()
  await run(bin!, [...args, 'seed-persona', '--persona', persona, '--namespace', namespace, '--out', outFile])
  const manifest: Manifest = JSON.parse(await readFile(outFile, 'utf8'))
  return {
    ...manifest,
    account(key) {
      const account = manifest.accounts[key]
      if (!account) {
        throw new Error(`the ${persona} persona has no account '${key}'; it has ${Object.keys(manifest.accounts).join(', ')}`)
      }
      return account
    },
  }
}

/** Storage state that carries `token` as the session cookie. */
export function storageStateFor(baseURL: string, token: string | undefined) {
  if (!token) return { cookies: [], origins: [] }
  const url = new URL(baseURL)
  return {
    cookies: [
      {
        name: 'bizkit_session',
        value: token,
        domain: url.hostname,
        path: '/',
        expires: -1,
        httpOnly: true,
        secure: url.protocol === 'https:',
        sameSite: 'Lax' as const,
      },
    ],
    origins: [],
  }
}
