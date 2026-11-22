import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { h } from 'vue'

import DataTable from './DataTable.vue'

const columns = [
  { key: 'name', label: 'Name' },
  { key: 'actions', label: 'Actions', hidden: true },
]

describe('DataTable', () => {
  it('renders rows through cell slots', () => {
    const wrapper = mount(DataTable<{ id: string; name: string }>, {
      props: { columns, rows: [{ id: '1', name: 'Site' }], rowKey: (r) => r.id },
      slots: { 'cell-name': ({ row }: { row: { name: string } }) => h('b', row.name) },
    })
    expect(wrapper.find('tbody b').text()).toBe('Site')
    expect(wrapper.find('th .sr-only').text()).toBe('Actions')
  })

  it('shows the empty state', () => {
    const wrapper = mount(DataTable<{ id: string }>, {
      props: { columns, rows: [], rowKey: (r) => r.id, empty: { title: 'Nothing here' } },
    })
    expect(wrapper.text()).toContain('Nothing here')
    expect(wrapper.find('table').exists()).toBe(false)
  })
})
