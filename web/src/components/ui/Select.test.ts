import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'

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

  it('keeps the list inside the viewport near the right edge', async () => {
    vi.spyOn(document.documentElement, 'clientWidth', 'get').mockReturnValue(375)
    vi.spyOn(document.documentElement, 'clientHeight', 'get').mockReturnValue(800)
    const wrapper = mount(Select, {
      props: { options, label: 'Role', modelValue: 'admin', size: 'sm' },
      attachTo: document.body,
    })
    const button = wrapper.get('[role="combobox"]')
    vi.spyOn(button.element, 'getBoundingClientRect').mockReturnValue(
      DOMRect.fromRect({ x: 290, y: 100, width: 80, height: 32 }),
    )
    await button.trigger('click')
    const list = document.querySelector<HTMLElement>('[role="listbox"]')
    expect(list?.style.left).toBe('143px')
    expect(list?.style.minWidth).toBe('224px')
    expect(list?.style.maxWidth).toBe('359px')
    wrapper.unmount()
    vi.restoreAllMocks()
  })
})
