import { PiniaColada } from '@pinia/colada'
import { flushPromises, mount, RouterLinkStub } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { describe, expect, it, vi } from 'vitest'

import BreakdownPanel from './BreakdownPanel.vue'

vi.mock('@/api/gen/sdk.gen', async (original) => ({
  ...(await original<typeof import('@/api/gen/sdk.gen')>()),
  analyticsStatsBreakdown: vi.fn(async () => ({
    data: {
      items: [{ value: 'signup', visitors: 3, pageviews: 0, events: 7 }],
    },
  })),
}))

function render(props: Record<string, unknown> = {}) {
  return mount(BreakdownPanel, {
    props: {
      workspaceId: 'w1',
      projectId: 'p1',
      range: { from: '2026-11-01', to: '2026-11-28' },
      title: 'Top events',
      dimensions: [{ value: 'event', label: 'Event' }],
      ...props,
    },
    global: { plugins: [createPinia(), PiniaColada], stubs: { RouterLink: RouterLinkStub } },
  })
}

describe('BreakdownPanel', () => {
  it('links Explore and shows the description', async () => {
    const wrapper = render({ explore: '/events', description: 'The most sent events.' })
    await flushPromises()
    const link = wrapper.getComponent(RouterLinkStub)
    expect(link.text()).toBe('Explore')
    expect(link.props('to')).toBe('/events')
    expect(wrapper.text()).toContain('The most sent events.')
    expect(wrapper.text()).toContain('signup')
  })

  it('has no Explore link without one', async () => {
    const wrapper = render()
    await flushPromises()
    expect(wrapper.findAllComponents(RouterLinkStub)).toHaveLength(0)
  })
})
