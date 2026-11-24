import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it } from 'vitest'
import { defineComponent, h, nextTick } from 'vue'

import Combobox from './Combobox.vue'
import Popover from './Popover.vue'

const Host = defineComponent({
  setup() {
    return () =>
      h(
        Popover,
        { label: 'Add filter' },
        {
          trigger: ({ toggle, controls }: { toggle: () => void; controls: string }) =>
            h('button', { type: 'button', 'aria-controls': controls, onClick: toggle }, 'Add'),
          default: ({ close }: { close: () => void }) => [
            h(Combobox, {
              options: [{ value: 'plan' }, { value: 'seats' }],
              label: 'Property',
              modelValue: '',
            }),
            h('button', { type: 'button', onClick: () => close() }, 'Done'),
          ],
        },
      )
  },
})

function dialog() {
  return document.querySelector<HTMLElement>('[role="dialog"]')
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('Popover', () => {
  it('opens with focus on its first field and gives it back on Escape', async () => {
    const wrapper = mount(Host, { attachTo: document.body })
    const trigger = wrapper.get('button')
    await trigger.trigger('click')
    await nextTick()
    expect(dialog()?.getAttribute('aria-label')).toBe('Add filter')
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Property')
    dialog()?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await nextTick()
    expect(dialog()).toBeNull()
    expect(document.activeElement).toBe(trigger.element)
    wrapper.unmount()
  })

  it('stays open while a list inside it handles Escape', async () => {
    const wrapper = mount(Host, { attachTo: document.body })
    await wrapper.get('button').trigger('click')
    await nextTick()
    const input = document.activeElement as HTMLInputElement
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    await nextTick()
    expect(document.querySelectorAll('[role="option"]')).toHaveLength(2)
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await nextTick()
    expect(document.querySelectorAll('[role="option"]')).toHaveLength(0)
    expect(dialog()).not.toBeNull()
    wrapper.unmount()
  })

  it('stays open when a list inside it is clicked, and closes on a click outside', async () => {
    const wrapper = mount(Host, { attachTo: document.body })
    await wrapper.get('button').trigger('click')
    await nextTick()
    const input = document.activeElement as HTMLInputElement
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    await nextTick()
    const option = document.querySelector('[role="option"]')!
    option.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    option.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await nextTick()
    expect(dialog()).not.toBeNull()
    // onClickOutside takes one click per task.
    await new Promise((resolve) => setTimeout(resolve, 0))
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await nextTick()
    expect(dialog()).toBeNull()
    wrapper.unmount()
  })
})
