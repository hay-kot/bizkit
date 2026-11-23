import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import BarList from './BarList.vue'

const router = createRouter({
  history: createMemoryHistory(),
  routes: [{ path: '/:any(.*)*', component: { template: '<div />' } }],
})

const items = [
  { key: '/', label: '/', value: 812, secondary: 900, to: '/pages/home' },
  { key: '/pricing', label: '/pricing', value: 301, secondary: 350 },
  { key: '/blog', label: '/blog', value: 240, secondary: 260 },
]

function mountList(props: Partial<InstanceType<typeof BarList>['$props']> = {}) {
  return mount(BarList, {
    props: { items, valueLabel: 'Visitors', ...props },
    global: { plugins: [router] },
  })
}

describe('BarList', () => {
  it('ranks items in an ordered list with share bars against the top value', () => {
    const wrapper = mountList({ secondaryLabel: 'Pageviews' })
    const rows = wrapper.findAll('ol > li')
    expect(rows).toHaveLength(3)
    expect(rows[0]!.text()).toContain('Visitors: 812')
    expect(rows[0]!.text()).toContain('Pageviews: 900')
    expect(rows[0]!.get('a').attributes('href')).toBe('/pages/home')
    expect(rows[1]!.find('a').exists()).toBe(false)
    const bars = wrapper.findAll('progress')
    expect(bars.map((b) => (b.element as HTMLProgressElement).value)).toEqual([812, 301, 240])
    expect(bars.every((b) => (b.element as HTMLProgressElement).max === 812)).toBe(true)
    expect(wrapper.find('button').exists()).toBe(false)
  })

  it('limits the rows and emits show-all', async () => {
    const wrapper = mountList({ limit: 2 })
    expect(wrapper.findAll('ol > li')).toHaveLength(2)
    await wrapper.get('button').trigger('click')
    expect(wrapper.emitted('show-all')).toHaveLength(1)
  })

  it('shows the empty text and the loading state', () => {
    expect(mountList({ items: [] }).text()).toBe('No data for this range')
    expect(mountList({ items: [], empty: 'No goals yet' }).text()).toBe('No goals yet')
    expect(mountList({ loading: true }).find('[aria-busy="true"]').exists()).toBe(true)
  })
})
