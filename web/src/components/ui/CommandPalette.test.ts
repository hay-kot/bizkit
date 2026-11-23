import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, nextTick } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'

import { recents } from '@/palette/recents'
import { usePaletteRows } from '@/palette/registry'
import { closePalette, openPalette, paletteOpen, paletteScope } from '@/palette/state'
import type { PaletteRow } from '@/palette/types'

import CommandPalette from './CommandPalette.vue'

const signOut = vi.fn()
const rows: PaletteRow[] = [
  { id: 'page:forms', title: 'Forms', group: 'Website', order: 10, scope: 'goto', to: '/forms' },
  { id: 'forms.form:1', title: 'Contact', group: 'Forms', order: 100, scope: 'goto', to: '/c' },
  { id: 'forms:new', title: 'New form', group: 'Forms', order: 100, to: '/forms?new=1' },
  { id: 'auth:sign-out', title: 'Sign out', group: 'Account', order: 900, run: signOut },
]

let scope: ReturnType<typeof effectScope>
let unmount: (() => void) | undefined

function setup() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/:any(.*)*', component: { template: '<div />' } }],
  })
  const wrapper = mount(CommandPalette, { global: { plugins: [router] }, attachTo: document.body })
  unmount = () => wrapper.unmount()
  return { router, wrapper }
}

const input = () => document.querySelector<HTMLInputElement>('[role="combobox"]')!
const options = () => [...document.querySelectorAll('[role="option"]')].map((o) => o.textContent)
const selected = () => document.querySelector('[role="option"][aria-selected="true"]')?.textContent

async function type(value: string) {
  input().value = value
  input().dispatchEvent(new Event('input'))
  await nextTick()
}

async function press(key: string, init: KeyboardEventInit = {}) {
  input().dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, ...init }))
  await nextTick()
}

beforeEach(() => {
  recents.value = []
  scope = effectScope()
  scope.run(() => usePaletteRows(rows))
})

afterEach(() => {
  closePalette()
  scope.stop()
  unmount?.()
  unmount = undefined
  signOut.mockReset()
})

describe('CommandPalette', () => {
  it('opens on the groups in placement order and focuses the input', async () => {
    setup()
    openPalette()
    await flushPromises()
    expect(document.activeElement).toBe(input())
    expect(options()).toEqual([
      expect.stringContaining('Forms'),
      expect.stringContaining('Contact'),
      expect.stringContaining('New form'),
      expect.stringContaining('Sign out'),
    ])
    expect(selected()).toContain('Forms')
  })

  it('filters by the query and draws the group path', async () => {
    setup()
    openPalette()
    await flushPromises()
    await type('contact')
    expect(options()).toHaveLength(1)
    expect(options()[0]).toContain('Forms ›')
    expect(options()[0]).toContain('Contact')
  })

  it('moves with the arrows, wraps, and runs the selection on Enter', async () => {
    const { router } = setup()
    openPalette()
    await flushPromises()
    await press('ArrowUp')
    expect(selected()).toContain('Sign out')
    await press('ArrowDown')
    await press('ArrowDown')
    expect(selected()).toContain('Contact')
    await press('Enter')
    await flushPromises()
    expect(paletteOpen.value).toBe(false)
    expect(router.currentRoute.value.fullPath).toBe('/c')
    expect(recents.value[0]).toBe('forms.form:1')
  })

  it('runs a run row', async () => {
    setup()
    openPalette()
    await flushPromises()
    await type('sign')
    await press('Enter')
    await flushPromises()
    expect(signOut).toHaveBeenCalledOnce()
  })

  it('enters Actions with the sigil, cycles with Tab and goes back with Backspace', async () => {
    setup()
    openPalette()
    await flushPromises()
    await type('>')
    expect(paletteScope.value).toBe('actions')
    expect(input().value).toBe('')
    expect(options()).toEqual([
      expect.stringContaining('New form'),
      expect.stringContaining('Sign out'),
    ])
    await press('Tab')
    expect(paletteScope.value).toBe('all')
    await press('Tab', { shiftKey: true })
    expect(paletteScope.value).toBe('actions')
    await press('Tab', { shiftKey: true })
    expect(paletteScope.value).toBe('goto')
    await press('Backspace')
    expect(paletteScope.value).toBe('all')
  })

  it('shows recents first on the empty All view only', async () => {
    recents.value = ['auth:sign-out']
    setup()
    openPalette()
    await flushPromises()
    expect(document.body.textContent).toContain('Recent')
    expect(options()[0]).toContain('Sign out')
    expect(options().filter((o) => o?.includes('Sign out'))).toHaveLength(1)
    await type('@')
    expect(document.body.textContent).not.toContain('Recent')
  })

  it('closes on Escape', async () => {
    setup()
    openPalette()
    await flushPromises()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await nextTick()
    expect(paletteOpen.value).toBe(false)
  })

  it('leaves a scroll lock it did not take', async () => {
    setup()
    document.body.style.overflow = 'hidden'
    openPalette()
    await flushPromises()
    closePalette()
    await flushPromises()
    expect(document.body.style.overflow).toBe('hidden')
    document.body.style.overflow = ''
  })
})
