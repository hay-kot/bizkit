import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'

import SignaturePad from './SignaturePad.vue'

function mountPad(modelValue?: unknown) {
  return mount(SignaturePad, {
    props: { label: 'signature', typedDefault: 'Ada Lovelace', modelValue: modelValue as never },
    attachTo: document.body,
  })
}

/** The last value the pad emitted. */
const last = (wrapper: ReturnType<typeof mountPad>) =>
  wrapper.emitted('update:modelValue')?.at(-1)?.[0]

describe('SignaturePad', () => {
  it('records strokes in pad units, pinned to the pad', async () => {
    const wrapper = mountPad()
    const canvas = wrapper.get('canvas')
    const el = canvas.element as HTMLCanvasElement
    vi.spyOn(el, 'getBoundingClientRect').mockReturnValue({
      left: 0,
      top: 0,
      width: 500,
      height: 150,
    } as DOMRect)
    el.setPointerCapture = () => {}
    await canvas.trigger('pointerdown', { clientX: 50, clientY: 30, pointerId: 1 })
    await canvas.trigger('pointermove', { clientX: 250, clientY: 75, pointerId: 1 })
    await canvas.trigger('pointermove', { clientX: 700, clientY: -20, pointerId: 1 })
    await canvas.trigger('pointerup', { pointerId: 1 })
    expect(last(wrapper)).toEqual({
      kind: 'drawn',
      strokes: [
        [
          [100, 60],
          [500, 150],
          [1000, 0],
        ],
      ],
    })
    wrapper.unmount()
  })

  it('needs two points before there is a drawing to adopt', async () => {
    const wrapper = mountPad()
    const canvas = wrapper.get('canvas')
    vi.spyOn(canvas.element, 'getBoundingClientRect').mockReturnValue({
      left: 0,
      top: 0,
      width: 1000,
      height: 300,
    } as DOMRect)
    ;(canvas.element as HTMLCanvasElement).setPointerCapture = () => {}
    await canvas.trigger('pointerdown', { clientX: 10, clientY: 10, pointerId: 1 })
    await canvas.trigger('pointerup', { pointerId: 1 })
    expect(last(wrapper)).toBeUndefined()
    wrapper.unmount()
  })

  it('offers the typed name, trimmed', async () => {
    const wrapper = mountPad()
    await wrapper.get('[role="radio"]:nth-child(2)').trigger('click')
    const input = wrapper.get('input[type="text"]')
    expect((input.element as HTMLInputElement).value).toBe('Ada Lovelace')
    expect(last(wrapper)).toEqual({ kind: 'typed', name: 'Ada Lovelace' })
    await input.setValue('  A. Lovelace ')
    expect(last(wrapper)).toEqual({ kind: 'typed', name: 'A. Lovelace' })
    await input.setValue('   ')
    expect(last(wrapper)).toBeUndefined()
    wrapper.unmount()
  })
})
