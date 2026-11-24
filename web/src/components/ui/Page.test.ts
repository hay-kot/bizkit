import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import Page from './Page.vue'

describe('Page', () => {
  it.each([
    [undefined, 'w-full'],
    ['compact', 'max-w-md'],
  ] as const)('width %s sets %s', (width, expected) => {
    const wrapper = mount(Page, { props: { width }, slots: { default: 'body' } })
    expect(wrapper.classes()).toContain(expected)
    expect(wrapper.text()).toBe('body')
  })

  it('wide pages have no max width', () => {
    const wrapper = mount(Page)
    expect(wrapper.classes().some((c) => c.startsWith('max-w-'))).toBe(false)
  })
})
