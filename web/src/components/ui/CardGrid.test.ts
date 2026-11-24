import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { h } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'

import CardGrid from './CardGrid.vue'
import LinkCard from './LinkCard.vue'

type Item = { id: string; name: string }

async function withRouter() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/:any(.*)*', component: { template: '<div />' } }],
  })
  await router.push('/home')
  return router
}

describe('CardGrid', () => {
  it('renders a cell per item and the end cell last', () => {
    const wrapper = mount(CardGrid<Item>, {
      props: {
        items: [
          { id: '1', name: 'Acme' },
          { id: '2', name: 'Globex' },
        ],
        itemKey: (i) => i.id,
      },
      slots: {
        item: ({ item }: { item: Item }) => h('b', item.name),
        end: () => h('button', 'New'),
      },
    })
    const cells = wrapper.findAll('li')
    expect(cells.map((c) => c.text())).toEqual(['Acme', 'Globex', 'New'])
  })

  it('shows skeleton cards while loading', () => {
    const wrapper = mount(CardGrid<Item>, {
      props: { items: undefined, itemKey: (i) => i.id, loading: true },
    })
    expect(wrapper.find('[aria-busy="true"]').exists()).toBe(true)
    expect(wrapper.find('ul').exists()).toBe(false)
  })
})

describe('LinkCard', () => {
  it('makes the title the one link over the card, with actions above it', async () => {
    const router = await withRouter()
    const onSettings = vi.fn()
    const wrapper = mount(LinkCard, {
      props: { to: '/w/1', title: 'Acme', description: 'acme' },
      slots: {
        actions: () => h('button', { type: 'button', class: 'settings', onClick: onSettings }),
      },
      global: { plugins: [router] },
    })
    const links = wrapper.findAll('a')
    expect(links).toHaveLength(1)
    expect(links[0]!.text()).toBe('Acme')
    expect(links[0]!.classes()).toEqual(
      expect.arrayContaining(['after:absolute', 'after:inset-0', 'text-primary']),
    )
    expect(wrapper.classes()).toContain('relative')

    const settings = wrapper.find('button.settings')
    expect(settings.element.parentElement!.classList).toContain('relative')
    await settings.trigger('click')
    await flushPromises()
    expect(onSettings).toHaveBeenCalledOnce()
    expect(router.currentRoute.value.fullPath).toBe('/home')
  })
})
