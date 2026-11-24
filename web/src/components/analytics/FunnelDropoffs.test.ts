import { mount, RouterLinkStub } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import { formatTime } from '@/lib/format'
import type { DropoffPage } from '@/queries/analytics'

import FunnelDropoffs from './FunnelDropoffs.vue'

const page: DropoffPage = {
  total: 3,
  items: [
    {
      person_id: 'p1',
      user_id: 'ada',
      display: 'ada@x.test',
      entered_at: '2026-01-01T10:00:00Z',
      last_step_at: '2026-01-01T10:05:00Z',
      events: [],
    },
    {
      entered_at: '2026-01-01T09:00:00Z',
      last_step_at: '2026-01-01T09:00:00Z',
      events: [
        {
          id: 7,
          ts: '2026-01-01T09:00:00Z',
          name: 'pageview',
          path: '/pricing',
          props: {},
          source: 'web',
        },
        { id: 8, ts: '2026-01-01T09:02:00Z', name: 'faq_opened', props: {}, source: 'web' },
      ],
    },
  ],
}

function render(props: Partial<InstanceType<typeof FunnelDropoffs>['$props']> = {}) {
  return mount(FunnelDropoffs, {
    props: {
      step: 3,
      label: 'signup',
      page,
      loading: false,
      error: undefined,
      personPath: (id: string) => `/people/${id}`,
      close: { query: {} },
      ...props,
    },
    global: { stubs: { RouterLink: RouterLinkStub } },
  })
}

describe('FunnelDropoffs', () => {
  it('links identified people and lists anonymous events', () => {
    const wrapper = render()
    expect(wrapper.text()).toContain('Dropped before step 3: signup')
    expect(wrapper.text()).toContain('the latest 2 are listed')
    const links = wrapper.findAllComponents(RouterLinkStub)
    expect(links.map((link) => link.props('to'))).toContain('/people/p1')
    const rows = wrapper.findAll('tbody tr')
    expect(rows[0]!.text()).toContain('ada@x.test')
    expect(rows[1]!.text()).toContain('Anonymous visitor')
    expect(rows[1]!.text()).toContain(`${formatTime('2026-01-01T09:00:00Z')}/pricing`)
    expect(rows[1]!.text()).toContain('faq_opened')
  })

  it('says when nobody dropped off', () => {
    const wrapper = render({ page: { total: 0, items: [] } })
    expect(wrapper.text()).toContain('Nobody dropped off here')
  })
})
