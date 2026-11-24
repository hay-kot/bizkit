import { PiniaColada } from '@pinia/colada'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'

import { analyticsSchemaEvents, analyticsSchemaValues } from '@/api/gen/sdk.gen'

import { eventOptions, useSchemaEvents, useSchemaValues } from './analytics'

vi.mock('@/api/gen/sdk.gen', async (original) => ({
  ...(await original<typeof import('@/api/gen/sdk.gen')>()),
  analyticsSchemaEvents: vi.fn(async () => ({
    data: { items: [{ name: 'pageview', events: 9, users: 3, last_seen: '' }], sampled: false },
  })),
  analyticsSchemaValues: vi.fn(async () => ({ data: { items: [], sampled: false } })),
}))

function harness(setup: () => unknown) {
  return defineComponent({
    setup() {
      setup()
      return () => h('div')
    },
  })
}

describe('schema queries', () => {
  beforeEach(() => vi.mocked(analyticsSchemaEvents).mockClear())

  it('asks once a minute for the same lookup', async () => {
    // One pinia for both mounts, so they share the query cache like two
    // pickers on one page.
    const global = { plugins: [createPinia(), PiniaColada] }
    const lookup = () =>
      useSchemaEvents('w1', 'p1', { from: '2026-01-01', to: '2026-01-30', q: 'sign' })
    mount(harness(lookup), { global })
    await flushPromises()
    mount(harness(lookup), { global })
    await flushPromises()
    expect(analyticsSchemaEvents).toHaveBeenCalledTimes(1)
    expect(vi.mocked(analyticsSchemaEvents).mock.calls[0]?.[0]).toMatchObject({
      path: { workspace_id: 'w1' },
      query: { project_id: 'p1', from: '2026-01-01', to: '2026-01-30', q: 'sign' },
    })
  })

  it('waits for a key before asking for values', async () => {
    mount(
      harness(() => useSchemaValues('w1', 'p1', { source: 'property', key: '' })),
      { global: { plugins: [createPinia(), PiniaColada] } },
    )
    await flushPromises()
    expect(analyticsSchemaValues).not.toHaveBeenCalled()
  })

  it('groups page views ahead of custom events', () => {
    const at = '2026-01-01T00:00:00Z'
    expect(
      eventOptions([
        { name: 'signup', events: 5, users: 2, last_seen: at },
        { name: 'pageview', events: 9, users: 3, last_seen: at },
      ]),
    ).toEqual([
      { value: 'pageview', count: 9, group: 'Page views' },
      { value: 'signup', count: 5, group: 'Custom events' },
    ])
  })
})
