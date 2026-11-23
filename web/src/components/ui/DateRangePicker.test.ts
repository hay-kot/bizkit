import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import type { DateRange } from '@/lib/range'

import DateRangePicker from './DateRangePicker.vue'

const today = '2026-03-15'

function mountPicker(modelValue: DateRange) {
  return mount(DateRangePicker, {
    props: { modelValue, today },
    attachTo: document.body,
  })
}

async function choose(wrapper: ReturnType<typeof mountPicker>, label: string) {
  await wrapper.get('[role="combobox"]').trigger('click')
  const option = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find(
    (o) => o.textContent?.trim() === label,
  )
  option!.click()
  await wrapper.vm.$nextTick()
}

describe('DateRangePicker', () => {
  it('turns a preset into its days', async () => {
    const wrapper = mountPicker({ preset: '30d', from: '2026-02-14', to: today })
    expect(wrapper.find('input[type="date"]').exists()).toBe(false)
    await choose(wrapper, 'Last 7 days')
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([
      { preset: '7d', from: '2026-03-09', to: today },
    ])
    wrapper.unmount()
  })

  it('keeps the dates when switching to custom', async () => {
    const wrapper = mountPicker({ preset: '7d', from: '2026-03-09', to: today })
    await choose(wrapper, 'Custom')
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([
      { preset: 'custom', from: '2026-03-09', to: today },
    ])
    wrapper.unmount()
  })

  it('edits custom dates and keeps from before to', async () => {
    const wrapper = mountPicker({ preset: 'custom', from: '2026-03-01', to: '2026-03-10' })
    const [from, to] = wrapper.findAll('input[type="date"]')
    expect(from!.attributes('aria-label')).toBe('From')
    expect(to!.attributes('max')).toBe(today)

    await from!.setValue('2026-03-05')
    await from!.trigger('change')
    await to!.setValue('2026-02-20')
    await to!.trigger('change')
    const emitted = wrapper.emitted('update:modelValue')!.map(([value]) => value)
    expect(emitted).toContainEqual({ preset: 'custom', from: '2026-03-05', to: '2026-03-10' })
    expect(emitted).toContainEqual({ preset: 'custom', from: '2026-02-20', to: '2026-02-20' })
    wrapper.unmount()
  })
})
