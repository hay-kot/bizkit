import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { defineComponent } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'

import { useReportRange } from './useReportRange'

async function mountRange() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div />' } }],
  })
  await router.push('/?range=7d')
  let state!: ReturnType<typeof useReportRange>
  const wrapper = mount(
    defineComponent({
      setup() {
        state = useReportRange()
        return () => null
      },
    }),
    { global: { plugins: [router] } },
  )
  return { wrapper, state: () => state }
}

afterEach(() => {
  vi.useRealTimers()
})

describe('useReportRange', () => {
  it('moves today and the presets at UTC midnight', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] })
    vi.setSystemTime(new Date('2026-03-15T23:59:00Z'))
    const { wrapper, state } = await mountRange()
    expect(state().today.value).toBe('2026-03-15')
    expect(state().range.value).toEqual({ preset: '7d', from: '2026-03-09', to: '2026-03-15' })

    vi.advanceTimersByTime(60_000)
    expect(state().today.value).toBe('2026-03-16')
    expect(state().range.value.to).toBe('2026-03-16')

    vi.advanceTimersByTime(86_400_000)
    expect(state().today.value).toBe('2026-03-17')
    wrapper.unmount()
    expect(vi.getTimerCount()).toBe(0)
  })

  it('checks the day again when the tab becomes visible', async () => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date('2026-03-15T12:00:00Z'))
    const { wrapper, state } = await mountRange()
    vi.setSystemTime(new Date('2026-03-17T08:00:00Z'))
    document.dispatchEvent(new Event('visibilitychange'))
    expect(state().today.value).toBe('2026-03-17')
    wrapper.unmount()
  })
})
