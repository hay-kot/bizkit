import { afterEach, describe, expect, it } from 'vitest'

import { runAction } from './action'
import { pendingConfirm } from './confirm'
import { toasts } from './toast'

afterEach(() => toasts.splice(0))

describe('runAction', () => {
  it('toasts success', async () => {
    expect(await runAction({ run: async () => 42, success: (n) => `got ${n}` })).toBe(42)
    expect(toasts.map((t) => t.message)).toEqual(['got 42'])
  })

  it('toasts API errors instead of throwing', async () => {
    const result = await runAction({
      run: () => Promise.reject({ error: { code: 'conflict', message: 'taken' } }),
    })
    expect(result).toBeUndefined()
    expect(toasts[0]).toMatchObject({ kind: 'error', message: 'taken' })
  })

  it('does nothing when the user cancels', async () => {
    let ran = false
    const pending = runAction({
      confirm: { title: 't', message: 'm' },
      run: async () => (ran = true),
    })
    pendingConfirm.value?.resolve(false)
    expect(await pending).toBeUndefined()
    expect(ran).toBe(false)
  })
})
