import { flushPromises, mount } from '@vue/test-utils'
import { ChartLine, FolderKanban } from 'lucide-vue-next'
import { describe, expect, it } from 'vitest'
import { ref } from 'vue'
import { createMemoryHistory, createRouter, RouterView } from 'vue-router'

import type { NavItem } from '@/modules/types'

import PageHeader from './PageHeader.vue'
import { PAGE_CONTEXT, type PageContext } from './pageHeader'

const forms: NavItem = {
  path: 'forms',
  label: 'Forms',
  icon: ChartLine,
  section: 'project',
  order: 100,
}
const projectName = ref<string | undefined>('Website')
const context: PageContext = {
  root: (projectId) => ({ label: projectName.value ?? '', to: `/w/1/p/${projectId}` }),
  navTo: (item, projectId) => `/w/1/p/${projectId}/${item.path}`,
}

async function mountAt(path: string, props: Record<string, unknown> = {}) {
  const page = { component: PageHeader, props: () => ({ title: 'Forms', ...props }) }
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/w/:ws/p/:projectId/forms/:rest(.*)*', ...page, meta: { nav: forms } },
      { path: '/:any(.*)*', ...page },
    ],
  })
  await router.push(path)
  const wrapper = mount(RouterView, {
    global: { plugins: [router], provide: { [PAGE_CONTEXT as symbol]: context } },
  })
  await flushPromises()
  return { router, wrapper }
}

type Wrapper = ReturnType<typeof mount>
const crumbs = (wrapper: Wrapper) =>
  wrapper.findAll('nav[aria-label="Breadcrumb"] li').map((li) => {
    if (li.find('[data-testid="crumb-skeleton"]').exists()) return '(loading)'
    const link = li.find('a')
    return link.exists()
      ? `${link.text()} -> ${link.attributes('href')}`
      : li.text().replace('/', '').trim()
  })

describe('PageHeader', () => {
  it('takes the icon and breadcrumb from the route module', async () => {
    const { wrapper } = await mountAt('/w/1/p/2/forms')
    expect(crumbs(wrapper)).toEqual(['Website -> /w/1/p/2', 'Forms'])
    expect(wrapper.find('[data-testid="page-icon"]').exists()).toBe(true)
  })

  it('decides the module page from the route, not the title', async () => {
    const { wrapper } = await mountAt('/w/1/p/2/forms/9', { title: 'Forms' })
    expect(crumbs(wrapper)).toEqual(['Website -> /w/1/p/2', 'Forms -> /w/1/p/2/forms', 'Forms'])
  })

  it('shows skeleton crumbs, not placeholders, while names load', async () => {
    projectName.value = undefined
    try {
      const { wrapper } = await mountAt('/w/1/p/2/forms/9', { title: '' })
      expect(crumbs(wrapper)).toEqual(['(loading)', 'Forms -> /w/1/p/2/forms', '(loading)'])
    } finally {
      projectName.value = 'Website'
    }
  })

  it('keeps its own crumbs and icon while the next route is current', async () => {
    // A leaving page stays mounted during the route transition.
    const { router } = await mountAt('/w/1/p/2/forms/9')
    const leaving = mount(PageHeader, {
      props: { title: 'Contact' },
      global: { plugins: [router], provide: { [PAGE_CONTEXT as symbol]: context } },
    })
    await router.push('/w/1/p/3/elsewhere')
    await flushPromises()
    expect(crumbs(leaving)).toEqual(['Website -> /w/1/p/2', 'Forms -> /w/1/p/2/forms', 'Contact'])
    expect(leaving.find('[data-testid="page-icon"]').exists()).toBe(true)
  })

  it('never links the last crumb, and takes explicit props over the route', async () => {
    const { wrapper } = await mountAt('/w/1/p/2/forms', {
      title: 'Projects',
      icon: FolderKanban,
      crumbs: [
        { label: 'Acme', to: '/w/1' },
        { label: 'Projects', to: '/w/1/projects' },
      ],
    })
    expect(crumbs(wrapper)).toEqual(['Acme -> /w/1', 'Projects'])
  })

  it('hides the breadcrumb and icon outside the shell', async () => {
    const wrapper = mount(PageHeader, { props: { title: 'Your workspaces' } })
    expect(wrapper.find('nav').exists()).toBe(false)
    expect(wrapper.find('[data-testid="page-icon"]').exists()).toBe(false)
  })
})
