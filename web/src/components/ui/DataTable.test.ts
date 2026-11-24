import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { h } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'

import DataTable from './DataTable.vue'

type Row = { id: string; name: string }

const columns = [
  { key: 'name', label: 'Name' },
  { key: 'actions', label: 'Actions', hidden: true, interactive: true },
]
const rows: Row[] = [
  { id: '1', name: 'Site' },
  { id: '2', name: 'Shop' },
]
const slots = {
  'cell-name': ({ row }: { row: Row }) => h('b', row.name),
  'cell-actions': ({ row }: { row: Row }) =>
    h('button', { type: 'button', class: 'delete', 'aria-label': `Delete ${row.name}` }),
}

async function withRouter() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/:any(.*)*', component: { template: '<div />' } }],
  })
  await router.push('/list')
  return router
}

describe('DataTable', () => {
  it('renders rows through cell slots', () => {
    const wrapper = mount(DataTable<Row>, {
      props: { columns, rows, rowKey: (r) => r.id },
      slots,
    })
    expect(wrapper.find('tbody b').text()).toBe('Site')
    expect(wrapper.find('th .sr-only').text()).toBe('Actions')
  })

  it('shows the empty state', () => {
    const wrapper = mount(DataTable<Row>, {
      props: { columns, rows: [], rowKey: (r) => r.id, empty: { title: 'Nothing here' } },
    })
    expect(wrapper.text()).toContain('Nothing here')
    expect(wrapper.find('table').exists()).toBe(false)
  })

  it('gives plain rows no link, pointer or hover', () => {
    const wrapper = mount(DataTable<Row>, {
      props: { columns, rows, rowKey: (r) => r.id },
      slots,
    })
    expect(wrapper.find('tbody a').exists()).toBe(false)
    const row = wrapper.find('tbody tr')
    expect(row.classes()).not.toContain('cursor-pointer')
    expect(row.classes()).not.toContain('hover:bg-base-200')
  })

  it('renders one real link per row in the primary cell, stretched over the row', async () => {
    const router = await withRouter()
    const wrapper = mount(DataTable<Row>, {
      props: { columns, rows, rowKey: (r) => r.id, rowTo: (r) => `/items/${r.id}` },
      slots,
      global: { plugins: [router] },
    })
    const tr = wrapper.findAll('tbody tr')
    expect(tr).toHaveLength(2)
    for (const [i, row] of tr.entries()) {
      const links = row.findAll('a')
      expect(links).toHaveLength(1)
      expect(links[0]!.attributes('href')).toBe(`/items/${rows[i]!.id}`)
      expect(links[0]!.text()).toBe(rows[i]!.name)
      // The hit area: the link's ::after fills the positioned row.
      expect(links[0]!.classes()).toEqual(
        expect.arrayContaining(['after:absolute', 'after:inset-0', 'text-primary']),
      )
      expect(links[0]!.classes()).not.toContain('link-hover')
      expect(row.classes()).toEqual(
        expect.arrayContaining(['relative', 'cursor-pointer', 'hover:bg-base-200']),
      )
      expect(row.attributes('role')).toBeUndefined()
      expect(row.attributes('tabindex')).toBeUndefined()
    }

    await wrapper.find('tbody a').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.fullPath).toBe('/items/1')
  })

  it('lifts interactive cells above the hit area, and their buttons do not navigate', async () => {
    const router = await withRouter()
    const onDelete = vi.fn()
    const wrapper = mount(DataTable<Row>, {
      props: { columns, rows, rowKey: (r) => r.id, rowTo: (r) => `/items/${r.id}` },
      slots: {
        ...slots,
        'cell-actions': () => h('button', { type: 'button', class: 'delete', onClick: onDelete }),
      },
      global: { plugins: [router] },
    })
    const actionCell = wrapper.findAll('tbody tr')[0]!.findAll('td')[1]!
    expect(actionCell.classes()).toContain('relative')
    expect(actionCell.find('a').exists()).toBe(false)

    await actionCell.find('button.delete').trigger('click')
    await flushPromises()
    expect(onDelete).toHaveBeenCalledOnce()
    expect(router.currentRoute.value.fullPath).toBe('/list')
  })

  it('leaves a row plain when rowTo returns undefined', async () => {
    const router = await withRouter()
    const wrapper = mount(DataTable<Row>, {
      props: {
        columns,
        rows,
        rowKey: (r) => r.id,
        rowTo: (r) => (r.id === '1' ? '/items/1' : undefined),
      },
      slots,
      global: { plugins: [router] },
    })
    const [first, second] = wrapper.findAll('tbody tr')
    expect(first!.find('a').exists()).toBe(true)
    expect(second!.find('a').exists()).toBe(false)
    expect(second!.classes()).not.toContain('cursor-pointer')
  })

  it('renders a button in the primary cell and fires row-click', async () => {
    const onRowClick = vi.fn()
    const wrapper = mount(DataTable<Row>, {
      props: { columns, rows, rowKey: (r) => r.id, onRowClick },
      slots,
    })
    const row = wrapper.findAll('tbody tr')[1]!
    const target = row.findAll('td')[0]!.find('button')
    expect(target.attributes('type')).toBe('button')
    expect(target.classes()).toEqual(expect.arrayContaining(['after:absolute', 'after:inset-0']))
    expect(row.classes()).toContain('cursor-pointer')

    await target.trigger('click')
    expect(onRowClick).toHaveBeenCalledWith(rows[1])

    await row.find('button.delete').trigger('click')
    expect(onRowClick).toHaveBeenCalledOnce()
  })

  it('puts the target in the column marked primary', async () => {
    const router = await withRouter()
    const wrapper = mount(DataTable<Row>, {
      props: {
        columns: [
          { key: 'actions', label: 'Actions' },
          { key: 'name', label: 'Name', primary: true },
        ],
        rows,
        rowKey: (r) => r.id,
        rowTo: (r) => `/items/${r.id}`,
      },
      slots,
      global: { plugins: [router] },
    })
    const cells = wrapper.findAll('tbody tr')[0]!.findAll('td')
    expect(cells[0]!.find('a').exists()).toBe(false)
    expect(cells[1]!.find('a').text()).toBe('Site')
  })
})
