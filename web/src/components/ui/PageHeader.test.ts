import { mount } from '@vue/test-utils'
import { ChartLine, FolderKanban } from 'lucide-vue-next'
import { describe, expect, it } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import type { NavItem } from '@/modules/types'

import PageHeader from './PageHeader.vue'
import { PAGE_CONTEXT, type PageContext } from './pageHeader'

const analytics: NavItem = {
  path: 'analytics',
  label: 'Analytics',
  icon: ChartLine,
  section: 'project',
  order: 102,
}
const context: PageContext = {
  root: { label: 'Website', to: '/w/1/p/2' },
  navTo: (item) => `/w/1/p/2/${item.path}`,
}

async function mountAt(path: string, props: Record<string, unknown>, slots = {}) {
  const page = { template: '<div />' }
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/w/1/p/2/analytics/:rest(.*)*', component: page, meta: { nav: analytics } },
      { path: '/:any(.*)*', component: page },
    ],
  })
  await router.push(path)
  return mount(PageHeader, {
    props: { title: 'Analytics', ...props },
    slots,
    global: { plugins: [router], provide: { [PAGE_CONTEXT as symbol]: context } },
  })
}

const crumbs = (wrapper: Awaited<ReturnType<typeof mountAt>>) =>
  wrapper.findAll('nav[aria-label="Breadcrumb"] li').map((li) => {
    const link = li.find('a')
    return link.exists()
      ? `${link.text()} -> ${link.attributes('href')}`
      : li.text().replace('/', '').trim()
  })

describe('PageHeader', () => {
  it('takes the icon and breadcrumb from the route module', async () => {
    const wrapper = await mountAt('/w/1/p/2/analytics', {})
    expect(crumbs(wrapper)).toEqual(['Website -> /w/1/p/2', 'Analytics'])
    expect(wrapper.find('[data-testid="page-icon"]').exists()).toBe(true)
    expect(wrapper.get('h1').text()).toBe('Analytics')
  })

  it('adds the title after the module on its other pages', async () => {
    const wrapper = await mountAt('/w/1/p/2/analytics/funnels/9', { title: 'Signup' })
    expect(crumbs(wrapper)).toEqual([
      'Website -> /w/1/p/2',
      'Analytics -> /w/1/p/2/analytics',
      'Signup',
    ])
  })

  it('never links the last crumb, and takes explicit props over the route', async () => {
    const wrapper = await mountAt('/w/1/p/2/analytics', {
      title: 'Projects',
      icon: FolderKanban,
      crumbs: [
        { label: 'Acme', to: '/w/1' },
        { label: 'Projects', to: '/w/1/projects' },
      ],
    })
    expect(crumbs(wrapper)).toEqual(['Acme -> /w/1', 'Projects'])
  })

  it('hides the breadcrumb and icon outside a module', async () => {
    const wrapper = await mountAt('/', { title: 'Your workspaces', crumbs: [] })
    expect(wrapper.find('nav').exists()).toBe(false)
    expect(wrapper.find('[data-testid="page-icon"]').exists()).toBe(false)
  })

  it('renders the docs link and the tabs slot', async () => {
    const wrapper = await mountAt(
      '/w/1/p/2/analytics',
      { description: 'Visitors.', docs: 'https://example.com/docs' },
      { tabs: '<nav aria-label="Tabs">tabs</nav>' },
    )
    expect(wrapper.get('a[target="_blank"]').attributes('href')).toBe('https://example.com/docs')
    expect(wrapper.get('header > div:last-child').text()).toBe('tabs')
  })
})
