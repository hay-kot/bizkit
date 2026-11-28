import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import Sparkline from './Sparkline.vue'

const part = (wrapper: ReturnType<typeof mount>, name: string) =>
  wrapper.find(`[data-part="${name}"]`)

describe('Sparkline', () => {
  it('draws a smooth solid line with an area', () => {
    const wrapper = mount(Sparkline, { props: { values: [1, 3, 2] } })
    expect(part(wrapper, 'solid').attributes('d')).toMatch(/^M0,\S+ C.* C/)
    expect(part(wrapper, 'area').attributes('d')).toMatch(/Z$/)
    expect(part(wrapper, 'partial').exists()).toBe(false)
    expect(wrapper.attributes('aria-hidden')).toBe('true')
  })

  it('turns dotted from the point before the open bucket', () => {
    const wrapper = mount(Sparkline, { props: { values: [1, 3, 2, 1], partialFrom: 3 } })
    const solid = part(wrapper, 'solid').attributes('d') ?? ''
    const partial = part(wrapper, 'partial')
    expect(solid.match(/C/g)).toHaveLength(2)
    expect(partial.attributes('d')?.match(/C/g)).toHaveLength(1)
    expect(partial.attributes('stroke-dasharray')).toBe('1 4')
  })

  it('draws the whole line dotted when the first bucket is open', () => {
    const wrapper = mount(Sparkline, { props: { values: [1, 2], partialFrom: 0 } })
    expect(part(wrapper, 'solid').exists()).toBe(false)
    expect(part(wrapper, 'partial').exists()).toBe(true)
  })

  it('draws one value as a dot and none as nothing', () => {
    expect(part(mount(Sparkline, { props: { values: [4] } }), 'dot').exists()).toBe(true)
    const empty = mount(Sparkline, { props: { values: [] } })
    expect(empty.findAll('path')).toHaveLength(0)
    expect(part(empty, 'dot').exists()).toBe(false)
  })

  it('keeps an all-zero line on the baseline', () => {
    const wrapper = mount(Sparkline, { props: { values: [0, 0], height: 40 } })
    expect(part(wrapper, 'solid').attributes('d')).toMatch(/^M0,38 /)
  })
})
