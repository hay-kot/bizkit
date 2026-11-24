import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import Tabs from './Tabs.vue'

const tabs = [
  { label: 'Overview', to: '/a', exact: true },
  { label: 'Funnels', to: '/a/funnels', also: ['/a/boards'] },
  { label: 'Settings', to: '/a/settings' },
]

async function mountAt(path: string) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/:any(.*)*', component: { template: '<div />' } }],
  })
  await router.push(path)
  const wrapper = mount(Tabs, {
    props: { tabs, label: 'Analytics' },
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

  it('keeps a tab active on its extra paths', async () => {
    const { wrapper } = await mountAt('/a/boards/7')
    expect(current(wrapper)).toEqual(['Funnels'])
  })

  it('navigates on click', async () => {
    const { router, wrapper } = await mountAt('/a')
    await wrapper.findAll('a')[2]!.trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/a/settings')
    expect(current(wrapper)).toEqual(['Settings'])
  })
})
