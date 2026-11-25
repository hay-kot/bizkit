import { describe, expect, it } from 'vitest'

import { isSignerSealing, nextSealPoll, type SigningSignerSession } from './signing'

const session = (allSigned: boolean, status: 'sent' | 'completed') =>
  ({
    all_signed: allSigned,
    request: { status },
    signer: { status: 'signed' },
  }) as SigningSignerSession

describe('the seal poll', () => {
  it('runs only while everyone signed and the request is still sent', () => {
    expect(isSignerSealing(session(true, 'sent'))).toBe(true)
    expect(isSignerSealing(session(false, 'sent')), 'others have yet to sign').toBe(false)
    expect(isSignerSealing(session(true, 'completed'))).toBe(false)
    expect(isSignerSealing(undefined)).toBe(false)
  })

  it('backs off from 2 s to 15 s and stops after 2 minutes', () => {
    const waits: number[] = []
    let waited = 0
    for (let attempt = 0; ; attempt++) {
      const wait = nextSealPoll(attempt, waited)
      if (wait === undefined) break
      waits.push(wait)
      waited += wait
    }
    expect(waits[0]).toBe(2_000)
    expect(waits.every((wait, i) => i === 0 || wait >= waits[i - 1]!)).toBe(true)
    expect(Math.max(...waits)).toBe(15_000)
    expect(waited).toBeGreaterThanOrEqual(120_000)
    expect(waited - waits.at(-1)!).toBeLessThan(120_000)
  })
})
