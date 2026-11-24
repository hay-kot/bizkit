import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { h } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'

import List from './List.vue'
import ListItem from './ListItem.vue'

type Item = { id: string; name: string }

const items: Item[] = [
  { id: '1', name: 'Site' },
  { id: '2', name: 'Shop' },
]

async function withRouter() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/:any(.*)*', component: { template: '<div />' } }],
  })
  await router.push('/list')
  return router
}

describe('List', () => {
  it('renders one list item per entry through the item slot', () => {
    const wrapper = mount(List<Item>, {
      props: { items, itemKey: (i) => i.id, label: 'Things' },
      slots: { item: ({ item }: { item: Item }) => h('b', item.name) },
    })
    expect(wrapper.find('ul').attributes('aria-label')).toBe('Things')
    expect(wrapper.findAll('li b').map((b) => b.text())).toEqual(['Site', 'Shop'])
  })

  it('shows the skeleton, the error or the empty state instead of the list', () => {
    const loading = mount(List<Item>, {
      props: { items: undefined, itemKey: (i) => i.id, loading: true },
    })
    expect(loading.find('[aria-busy="true"]').exists()).toBe(true)
    expect(loading.find('ul').exists()).toBe(false)

    const failed = mount(List<Item>, {
      props: { items: undefined, itemKey: (i) => i.id, error: new Error('Boom') },
    })
    expect(failed.text()).toContain('Boom')
    expect(failed.find('ul').exists()).toBe(false)

    const empty = mount(List<Item>, {
      props: { items: [], itemKey: (i) => i.id, empty: { title: 'Nothing here' } },
      slots: { 'empty-action': () => h('button', 'Add one') },
    })
    expect(empty.text()).toContain('Nothing here')
    expect(empty.find('button').text()).toBe('Add one')
    expect(empty.find('ul').exists()).toBe(false)
  })

  it('renders nothing for no items without an empty state', () => {
    const wrapper = mount(List<Item>, { props: { items: [], itemKey: (i) => i.id } })
    expect(wrapper.find('ul').exists()).toBe(false)
    expect(wrapper.find('ol').exists()).toBe(false)
    expect(wrapper.text()).toBe('')
  })

  it('renders an ordered list when ordered', () => {
    const wrapper = mount(List<Item>, {
      props: { items, itemKey: (i) => i.id, ordered: true },
      slots: { item: ({ item }: { item: Item }) => h('b', item.name) },
    })
    expect(wrapper.find('ul').exists()).toBe(false)
    expect(wrapper.findAll('ol > li')).toHaveLength(2)
  })
})

describe('ListItem', () => {
  it('is plain without to or select: no link, button, pointer or accent', () => {
    const wrapper = mount(ListItem, { props: { title: 'Ada', description: 'Admin' } })
    expect(wrapper.find('a').exists()).toBe(false)
    expect(wrapper.find('button').exists()).toBe(false)
    expect(wrapper.classes()).not.toContain('cursor-pointer')
    expect(wrapper.find('.text-primary').exists()).toBe(false)
    expect(wrapper.text()).toContain('Admin')
  })

  it('with to, renders the title as the one link, stretched over the row', async () => {
    const router = await withRouter()
    const onDelete = vi.fn()
    const wrapper = mount(ListItem, {
      props: { title: 'Site', to: '/items/1' },
      slots: {
        trailing: () => h('button', { type: 'button', class: 'delete', onClick: onDelete }),
      },
      global: { plugins: [router] },
    })
    const links = wrapper.findAll('a')
    expect(links).toHaveLength(1)
    expect(links[0]!.text()).toBe('Site')
    expect(links[0]!.attributes('href')).toBe('/items/1')
    expect(links[0]!.classes()).toEqual(
      expect.arrayContaining(['after:absolute', 'after:inset-0', 'text-primary']),
    )
    expect(wrapper.classes()).toEqual(
      expect.arrayContaining(['relative', 'cursor-pointer', 'hover:bg-base-200']),
    )

    // Trailing actions sit above the link's hit area and don't navigate.
    const trailing = wrapper.find('button.delete')
    expect(trailing.element.parentElement!.classList).toContain('relative')
    await trailing.trigger('click')
    await flushPromises()
    expect(onDelete).toHaveBeenCalledOnce()
    expect(router.currentRoute.value.fullPath).toBe('/list')

    await links[0]!.trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.fullPath).toBe('/items/1')
  })

  it('with select, renders the title as a button that fires it', async () => {
    const onSelect = vi.fn()
    const wrapper = mount(ListItem, { props: { title: 'From Ada', onSelect } })
    const button = wrapper.find('button')
    expect(button.attributes('type')).toBe('button')
    expect(button.text()).toBe('From Ada')
    expect(button.classes()).toContain('after:absolute')
    await button.trigger('click')
    expect(onSelect).toHaveBeenCalledOnce()
  })

  it('lifts a selectable description above the hit area, and only then', async () => {
    const router = await withRouter()
    const plain = mount(ListItem, {
      props: { title: 'Site', description: 'site', to: '/items/1' },
      global: { plugins: [router] },
    })
    const lifted = mount(ListItem, {
      props: { title: 'Site', description: 'site', to: '/items/1', selectableDescription: true },
      global: { plugins: [router] },
    })
    const description = (w: typeof plain) => w.findAll('span').find((s) => s.text() === 'site')!
    expect(description(plain).classes()).not.toContain('relative')
    expect(description(lifted).classes()).toContain('relative')
  })

  it('names the target with label when given', () => {
    const wrapper = mount(ListItem, {
      props: { title: 'Ada', label: 'Ada, received Nov 22', onSelect: () => {} },
    })
    expect(wrapper.find('button').attributes('aria-label')).toBe('Ada, received Nov 22')
  })

  it('puts the title slot inside the link', async () => {
    const router = await withRouter()
    const wrapper = mount(ListItem, {
      props: { to: '/items/1' },
      slots: { title: () => h('code', 'API_KEY') },
      global: { plugins: [router] },
    })
    expect(wrapper.find('a code').text()).toBe('API_KEY')
  })
})
