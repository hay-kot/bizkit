import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'

import Combobox, { type ComboOption } from './Combobox.vue'

const options: ComboOption[] = [
  { value: 'pageview', count: 1200, group: 'Page views' },
  { value: 'signup', count: 40, group: 'Custom events' },
  { value: 'signin', label: 'Sign in', group: 'Custom events' },
]

function setup(props: Record<string, unknown> = {}) {
  const wrapper = mount(Combobox, {
    props: { options, label: 'Event', modelValue: '', ...props },
    attachTo: document.body,
  })
  return { wrapper, input: wrapper.get('input') }
}

function listed(): string[] {
  return [...document.querySelectorAll('[role="option"]')].map((o) => o.textContent?.trim() ?? '')
}

afterEach(() => {
  document.body.innerHTML = ''
  vi.useRealTimers()
  vi.restoreAllMocks()
})

describe('Combobox', () => {
  it('follows the ARIA combobox pattern', async () => {
    const { input } = setup()
    expect(input.attributes('role')).toBe('combobox')
    expect(input.attributes('aria-autocomplete')).toBe('list')
    expect(input.attributes('aria-expanded')).toBe('false')
    await input.trigger('keydown', { key: 'ArrowDown' })
    expect(input.attributes('aria-expanded')).toBe('true')
    const list = document.querySelector('[role="listbox"]')
    expect(list?.id).toBe(input.attributes('aria-controls'))
    expect(input.attributes('aria-activedescendant')).toBe(
      document.querySelector('[role="option"]')?.id,
    )
  })

  it('groups options with their counts', async () => {
    const { input } = setup()
    await input.trigger('click')
    const groups = [...document.querySelectorAll('[role="group"]')]
    expect(groups).toHaveLength(2)
    expect(
      groups.map((g) =>
        document.getElementById(g.getAttribute('aria-labelledby') ?? '')?.textContent?.trim(),
      ),
    ).toEqual(['Page views', 'Custom events'])
    expect(listed()).toEqual(['pageview1,200', 'signup40', 'Sign in'])
  })

  it('filters as you type and picks with the keyboard', async () => {
    const { wrapper, input } = setup()
    await input.setValue('SIG')
    expect(listed()).toEqual(['signup40', 'Sign in'])
    await input.trigger('keydown', { key: 'ArrowDown' })
    await input.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['signin'])
    expect(input.attributes('aria-expanded')).toBe('false')
    await wrapper.setProps({ modelValue: 'signin' })
    expect((input.element as HTMLInputElement).value).toBe('Sign in')
  })

  it('shows every option when it opens on a chosen value', async () => {
    const { input } = setup({ modelValue: 'signup' })
    expect((input.element as HTMLInputElement).value).toBe('signup')
    await input.trigger('click')
    expect(listed()).toHaveLength(3)
    expect(document.querySelector('[aria-selected="true"]')?.textContent).toContain('signup')
  })

  it('closes on Escape and puts the chosen label back', async () => {
    const { wrapper, input } = setup({ modelValue: 'signin' })
    await input.setValue('xyz')
    expect(document.body.textContent).toContain('No matches')
    await input.trigger('keydown', { key: 'Escape' })
    expect(input.attributes('aria-expanded')).toBe('false')
    expect((input.element as HTMLInputElement).value).toBe('Sign in')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
  })

  it('offers typed text with allowCustom', async () => {
    const { wrapper, input } = setup({ allowCustom: true })
    await input.setValue('trial_started')
    expect(listed()).toEqual(['Use “trial_started”'])
    await input.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['trial_started'])
  })

  it('keeps typed text on blur with allowCustom, and drops it without', async () => {
    const custom = setup({ allowCustom: true })
    await custom.input.setValue('new_event')
    await custom.input.trigger('blur')
    expect(custom.wrapper.emitted('update:modelValue')?.[0]).toEqual(['new_event'])

    const strict = setup({ modelValue: 'signup' })
    await strict.input.setValue('new_event')
    await strict.input.trigger('blur')
    expect(strict.wrapper.emitted('update:modelValue')).toBeUndefined()
    expect((strict.input.element as HTMLInputElement).value).toBe('signup')
  })

  it('debounces an async lookup and keeps only the latest answer', async () => {
    vi.useFakeTimers()
    const lookup = vi.fn(async (q: string): Promise<ComboOption[]> => [{ value: `${q}-hit` }])
    const { input } = setup({ options: lookup })
    await input.trigger('click')
    await flushPromises()
    expect(lookup).toHaveBeenCalledWith('')
    await input.setValue('a')
    await input.setValue('ab')
    expect(lookup).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(150)
    await flushPromises()
    expect(lookup).toHaveBeenCalledTimes(2)
    expect(lookup).toHaveBeenLastCalledWith('ab')
    expect(listed()).toEqual(['ab-hit'])
  })

  it('keeps the list inside the viewport near the right edge', async () => {
    vi.spyOn(document.documentElement, 'clientWidth', 'get').mockReturnValue(375)
    vi.spyOn(document.documentElement, 'clientHeight', 'get').mockReturnValue(800)
    const { input } = setup({ size: 'sm' })
    vi.spyOn(input.element, 'getBoundingClientRect').mockReturnValue(
      DOMRect.fromRect({ x: 290, y: 100, width: 80, height: 32 }),
    )
    await input.trigger('click')
    const list = document.querySelector<HTMLElement>('[role="listbox"]')?.parentElement
    expect(list?.style.left).toBe('143px')
    expect(list?.style.top).toBe('136px')
  })
})
