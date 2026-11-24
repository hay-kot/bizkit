import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, describe, expect, it } from 'vitest'
import { h } from 'vue'

import type { DateRange } from '@/lib/range'

import TimeRangePicker from './TimeRangePicker.vue'

const today = '2026-03-15'
type Picker = VueWrapper<InstanceType<typeof TimeRangePicker>>
let wrapper: Picker | undefined

function mountPicker(range: DateRange, slot?: () => unknown): Picker {
  wrapper = mount(TimeRangePicker, {
    props: { range, today },
    slots: slot ? { default: slot } : {},
    attachTo: document.body,
  }) as Picker
  return wrapper
}

function button(name: string): HTMLButtonElement {
  const found = [...document.querySelectorAll<HTMLButtonElement>('button')].find(
    (b) => b.getAttribute('aria-label') === name || b.textContent?.trim() === name,
  )
  if (!found) throw new Error(`no button ${name}`)
  return found
}

function trigger(picker: Picker) {
  return picker.get('[aria-haspopup="dialog"]')
}

async function openPanel(picker: Picker) {
  await trigger(picker).trigger('click')
  await flushPromises()
}

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
})

describe('TimeRangePicker', () => {
  it('labels a preset by name and other ranges by their days', () => {
    expect(trigger(mountPicker({ preset: '30d', from: '2026-02-14', to: today })).text()).toBe(
      'Time range: Last 30 days',
    )
    wrapper!.unmount()
    expect(
      trigger(mountPicker({ preset: 'custom', from: '2026-03-01', to: '2026-03-07' })).text(),
    ).toBe('Time range: Mar 1 to Mar 7, 2026')
  })

  it('shifts by the range length and disables Next at today', async () => {
    const picker = mountPicker({ preset: '7d', from: '2026-03-09', to: today })
    expect(button('Next period').disabled).toBe(true)
    button('Previous period').click()
    await picker.vm.$nextTick()
    expect(picker.emitted('apply')?.[0]).toEqual([
      { preset: 'custom', from: '2026-03-02', to: '2026-03-08' },
    ])

    await picker.setProps({ range: { preset: 'custom', from: '2026-03-02', to: '2026-03-08' } })
    expect(button('Next period').disabled).toBe(false)
    button('Next period').click()
    expect(picker.emitted('apply')?.[1]).toEqual([
      { preset: 'custom', from: '2026-03-09', to: today },
    ])
  })

  it('zooms out to twice the days, ending today at the latest', () => {
    const picker = mountPicker({ preset: '7d', from: '2026-03-09', to: today })
    button('Zoom out').click()
    expect(picker.emitted('apply')?.[0]).toEqual([
      { preset: 'custom', from: '2026-03-02', to: today },
    ])
  })

  it('disables Zoom out at the API limit', () => {
    mountPicker({ preset: 'custom', from: '2025-02-09', to: today })
    expect(button('Zoom out').disabled).toBe(true)
  })

  it('refuses an absolute range longer than the API allows', async () => {
    const picker = mountPicker({ preset: '30d', from: '2026-02-14', to: today })
    await openPanel(picker)
    const from = document.querySelector<HTMLInputElement>('input[type="date"]')!
    from.value = '2025-02-08'
    from.dispatchEvent(new Event('input'))
    await picker.vm.$nextTick()
    expect(document.querySelector('[role="alert"]')?.textContent?.trim()).toBe(
      'Pick 400 days at most.',
    )
    expect(button('Apply').disabled).toBe(true)

    from.value = '2025-02-09'
    from.dispatchEvent(new Event('input'))
    await picker.vm.$nextTick()
    expect(button('Apply').disabled).toBe(false)
  })

  it('picks a quick range, marks the active one and returns focus', async () => {
    const picker = mountPicker({ preset: '30d', from: '2026-02-14', to: today })
    await openPanel(picker)
    const dialog = document.querySelector('[role="dialog"]')!
    const active = dialog.querySelector<HTMLElement>('[aria-current="true"]')!
    expect(active.textContent?.trim()).toBe('Last 30 days')
    expect(document.activeElement).toBe(active)

    button('Last 14 days').click()
    await picker.vm.$nextTick()
    expect(picker.emitted('apply')?.[0]).toEqual([{ preset: '14d', from: '2026-03-02', to: today }])
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(trigger(picker).element)
  })

  it('applies an absolute range and refuses one that ends after today', async () => {
    const picker = mountPicker({ preset: '30d', from: '2026-02-14', to: today })
    await openPanel(picker)
    const [from, to] = [...document.querySelectorAll<HTMLInputElement>('input[type="date"]')]
    expect(from!.value).toBe('2026-02-14')

    to!.value = '2026-03-20'
    to!.dispatchEvent(new Event('input'))
    await picker.vm.$nextTick()
    expect(button('Apply').disabled).toBe(true)

    from!.value = '2026-01-01'
    from!.dispatchEvent(new Event('input'))
    to!.value = '2026-01-31'
    to!.dispatchEvent(new Event('input'))
    await picker.vm.$nextTick()
    from!.form!.requestSubmit()
    await picker.vm.$nextTick()
    expect(picker.emitted('apply')?.[0]).toEqual([
      { preset: 'custom', from: '2026-01-01', to: '2026-01-31' },
    ])
  })

  it('closes on Escape and gives focus back to the trigger', async () => {
    const picker = mountPicker({ preset: '30d', from: '2026-02-14', to: today })
    await openPanel(picker)
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await picker.vm.$nextTick()
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(trigger(picker).element)
    expect(picker.emitted('apply')).toBeUndefined()
  })

  it('puts page controls after a divider', () => {
    const picker = mountPicker({ preset: '30d', from: '2026-02-14', to: today }, () =>
      h('button', { type: 'button' }, 'Compare'),
    )
    expect(picker.find('span[aria-hidden="true"].w-px').exists()).toBe(true)
    expect(picker.text()).toContain('Compare')
  })

  it('keeps the absolute fields in step with the range while open', async () => {
    const picker = mountPicker({ preset: '7d', from: '2026-03-09', to: today })
    await openPanel(picker)
    await picker.setProps({ range: { preset: 'custom', from: '2026-03-02', to: '2026-03-08' } })
    const [from, to] = [...document.querySelectorAll<HTMLInputElement>('input[type="date"]')]
    expect([from!.value, to!.value]).toEqual(['2026-03-02', '2026-03-08'])

    from!.form!.requestSubmit()
    expect(picker.emitted('apply')?.[0]).toEqual([
      { preset: 'custom', from: '2026-03-02', to: '2026-03-08' },
    ])
  })

  it('asks for both days once a field is cleared, not before', async () => {
    const picker = mountPicker({ preset: '7d', from: '2026-03-09', to: today })
    await openPanel(picker)
    expect(document.querySelector('[role="alert"]')).toBeNull()

    const from = document.querySelector<HTMLInputElement>('input[type="date"]')!
    from.value = ''
    from.dispatchEvent(new Event('input'))
    await picker.vm.$nextTick()
    expect(document.querySelector('[role="alert"]')?.textContent?.trim()).toBe('Pick both days.')
    expect(button('Apply').disabled).toBe(true)
  })

  it('hides a stale error when the range changes while open', async () => {
    const picker = mountPicker({ preset: '7d', from: '2026-03-09', to: today })
    await openPanel(picker)
    const from = document.querySelector<HTMLInputElement>('input[type="date"]')!
    from.value = ''
    from.dispatchEvent(new Event('input'))
    await picker.vm.$nextTick()
    expect(document.querySelector('[role="alert"]')).not.toBeNull()

    await picker.setProps({ range: { preset: 'custom', from: '2026-03-02', to: '2026-03-08' } })
    expect(document.querySelector('[role="alert"]')).toBeNull()
  })
})
