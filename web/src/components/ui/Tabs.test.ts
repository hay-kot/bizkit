import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import Tabs, { type Tab } from './Tabs.vue'

const tabs: Tab[] = [
  { label: 'Overview', to: '/a', exact: true },
  { label: 'Funnels', to: '/a/funnels' },
  { label: 'Settings', to: '/a/settings' },
]

async function mountAt(path: string, list: Tab[] = tabs) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/:any(.*)*', component: { template: '<div />' } }],
  })
  await router.push(path)
  const wrapper = mount(Tabs, {
    props: { tabs: list, label: 'Analytics' },
    global: { plugins: [router] },
  })
  return { router, wrapper }
}

const current = (wrapper: Awaited<ReturnType<typeof mountAt>>['wrapper']) =>
  wrapper.findAll('a[aria-current="page"]').map((a) => a.text())

describe('Tabs', () => {
  it('renders links in a labelled nav', async () => {
    const { wrapper } = await mountAt('/a')
    expect(wrapper.get('nav').attributes('aria-label')).toBe('Analytics')
    expect(wrapper.findAll('a').map((a) => a.attributes('href'))).toEqual([
      '/a',
      '/a/funnels',
      '/a/settings',
    ])
    expect(current(wrapper)).toEqual(['Overview'])
    expect(wrapper.get('a.tab-active').text()).toBe('Overview')
  })

  it('keeps a tab active on paths under it, but not an exact one', async () => {
    const { wrapper } = await mountAt('/a/funnels/42')
    expect(current(wrapper)).toEqual(['Funnels'])
  })

  it('navigates on click', async () => {
    const { router, wrapper } = await mountAt('/a')
    await wrapper.findAll('a')[2]!.trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/a/settings')
    expect(current(wrapper)).toEqual(['Settings'])
  })

  it('stays active on its `also` paths', async () => {
    const { wrapper } = await mountAt('/a/dashboards/7', [
      ...tabs,
      { label: 'Insights', to: '/a/insights', also: ['/a/dashboards'] },
    ])
    expect(current(wrapper)).toEqual(['Insights'])
  })

  it('shows a count after the label', async () => {
    const { wrapper } = await mountAt('/a', [{ label: 'Overview', to: '/a', count: 1200 }])
    expect(wrapper.get('a .badge').text()).toBe('1,200')
  })
})
