import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { h, nextTick } from 'vue'

import ActionsMenu from './ActionsMenu.vue'
import MenuItem from './MenuItem.vue'

function mountMenu(onSelect: () => void = () => {}) {
  return mount(ActionsMenu, {
    slots: { default: () => h(MenuItem, { danger: true, onSelect }, () => 'Delete') },
    attachTo: document.body,
  })
}

describe('ActionsMenu', () => {
  it('moves focus into the menu when it opens', async () => {
    const wrapper = mountMenu()
    await wrapper.get('button[aria-haspopup="menu"]').trigger('click')
    await nextTick()
    expect(document.activeElement?.textContent).toBe('Delete')
    expect(document.activeElement?.classList).toContain('text-error')
    wrapper.unmount()
  })

  it('gives focus back to the trigger before an item runs', async () => {
    let focusedDuringSelect: Element | null = null
    const wrapper = mountMenu(() => (focusedDuringSelect = document.activeElement))
    const trigger = wrapper.get('button[aria-haspopup="menu"]')
    await trigger.trigger('click')
    await nextTick()
    await wrapper.get('[role="menuitem"]').trigger('click')
    expect(focusedDuringSelect).toBe(trigger.element)
    expect(trigger.attributes('aria-expanded')).toBe('false')
    wrapper.unmount()
  })

  it('returns focus to the trigger on Escape', async () => {
    const wrapper = mountMenu()
    const trigger = wrapper.get('button[aria-haspopup="menu"]')
    await trigger.trigger('click')
    await nextTick()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await nextTick()
    expect(document.activeElement).toBe(trigger.element)
    expect(trigger.attributes('aria-expanded')).toBe('false')
    wrapper.unmount()
  })
})
