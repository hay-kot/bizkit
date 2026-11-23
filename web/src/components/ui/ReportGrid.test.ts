import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import ReportGrid from './ReportGrid.vue'

describe('ReportGrid', () => {
  it('lays panels out one column, two on wide screens', () => {
    const wrapper = mount(ReportGrid, {
      slots: { default: '<section>A</section><section>B</section>' },
    })
    expect(wrapper.classes()).toEqual(expect.arrayContaining(['grid-cols-1', 'lg:grid-cols-2']))
    expect(wrapper.findAll('section')).toHaveLength(2)
  })
})
