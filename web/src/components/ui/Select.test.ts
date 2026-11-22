import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import Select from './Select.vue'

const options = [
  { value: 'admin', label: 'Admin' },
  { value: 'member', label: 'Member' },
]

describe('Select', () => {
  it('picks with the keyboard', async () => {
    const wrapper = mount(Select, {
      props: { options, label: 'Role', modelValue: 'admin' },
      attachTo: document.body,
    })
    const button = wrapper.get('[role="combobox"]')
    await button.trigger('keydown', { key: 'ArrowDown' })
    expect(button.attributes('aria-expanded')).toBe('true')
    await button.trigger('keydown', { key: 'ArrowDown' })
    await button.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['member'])
    expect(button.attributes('aria-expanded')).toBe('false')
    wrapper.unmount()
  })

  it('closes on Escape without changing the value', async () => {
    const wrapper = mount(Select, {
      props: { options, label: 'Role', modelValue: 'admin' },
      attachTo: document.body,
    })
    const button = wrapper.get('[role="combobox"]')
    await button.trigger('click')
    await button.trigger('keydown', { key: 'Escape' })
    expect(button.attributes('aria-expanded')).toBe('false')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    wrapper.unmount()
  })
})
