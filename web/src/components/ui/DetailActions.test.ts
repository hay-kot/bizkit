import { mount } from '@vue/test-utils'
import { Download } from 'lucide-vue-next'
import { describe, expect, it, vi } from 'vitest'
import { h } from 'vue'

import DetailActions from './DetailActions.vue'
import MenuItem from './MenuItem.vue'

const save = { form: 'f', dirty: false, pending: false }

describe('DetailActions', () => {
  it.each([
    [{ ...save }, 'Saved', true],
    [{ ...save, dirty: true }, 'Save', false],
    [{ ...save, dirty: true, pending: true }, 'Saving', true],
    [{ ...save, dirty: true, blocked: 'Fix the test cases to save' }, 'Save', true],
  ])('save %j reads %s', (state, label, disabled) => {
    const wrapper = mount(DetailActions, { props: { save: state } })
    const button = wrapper.get('button[type="submit"]')
    expect(button.text()).toBe(label)
    expect(button.attributes('disabled') !== undefined).toBe(disabled)
    expect(button.attributes('form')).toBe('f')
  })

  it('announces the save state and why Save is off', () => {
    const wrapper = mount(DetailActions, {
      props: { save: { ...save, dirty: true, blocked: 'Fix the test cases to save' } },
    })
    expect(wrapper.get('[role="status"]').text()).toBe('Unsaved changes')
    const id = wrapper.get('button[type="submit"]').attributes('aria-describedby')
    expect(wrapper.get(`#${CSS.escape(id!)}`).text()).toBe('Fix the test cases to save')
  })

  it('gives every button the same size', () => {
    const run = vi.fn()
    const wrapper = mount(DetailActions, {
      props: { actions: [{ label: 'Export', icon: Download, run }], save },
      slots: { menu: () => h(MenuItem, { danger: true }, () => 'Delete') },
    })
    const buttons = wrapper.findAll('button')
    expect(buttons.map((b) => b.text())).toEqual(['Export', 'Saved', ''])
    for (const button of buttons)
      expect(button.classes()).toEqual(expect.arrayContaining(['btn', 'btn-sm']))
    expect(buttons.some((b) => b.classes().includes('btn-ghost'))).toBe(false)
    buttons[0]!.trigger('click')
    expect(run).toHaveBeenCalledWith()
  })

  it('leaves the menu out without items', () => {
    const wrapper = mount(DetailActions, { props: { save } })
    expect(wrapper.find('[aria-haspopup="menu"]').exists()).toBe(false)
  })
})
