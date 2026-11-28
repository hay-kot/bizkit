import { mount, RouterLinkStub } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import type { StepResult } from '@/queries/analytics'

import FunnelChart from './FunnelChart.vue'

function step(match: string, people: number, fromPrevious: number, fromFirst: number): StepResult {
  return {
    step: { kind: 'event', match },
    people,
    from_previous: fromPrevious,
    from_first: fromFirst,
  }
}

const three = [step('visit', 3, 1, 1), step('signup', 2, 2 / 3, 2 / 3), step('pay', 1, 0.5, 1 / 3)]

function render(steps: StepResult[], selected?: number) {
  return mount(FunnelChart, {
    props: { steps, droppedLink: (n: number) => `/dropped/${n}`, selected },
    global: { stubs: { RouterLink: RouterLinkStub } },
  })
}

describe('FunnelChart', () => {
  // Without layout the plot is 600px wide: three steps leave room for chips.
  it('links each drop-off from the chip between bars', () => {
    const wrapper = render(three, 3)
    const chips = wrapper.findAll('[data-part="chip"]')
    expect(chips).toHaveLength(2)
    expect(chips[0]!.text()).toContain('66.7%')
    const links = wrapper.findAllComponents(RouterLinkStub)
    expect(links.map((link) => link.text())).toEqual(['1 dropped off', '1 dropped off'])
    expect(links.map((link) => link.props('to'))).toEqual(['/dropped/2', '/dropped/3'])
    expect(links[1]!.attributes('aria-current')).toBe('true')
    expect(wrapper.find('ol').findAllComponents(RouterLinkStub)).toHaveLength(0)
  })

  it('hatches the lost share above each later bar', () => {
    const wrapper = render(three)
    expect(wrapper.findAll('[data-part="kept"]')).toHaveLength(3)
    expect(wrapper.findAll('[data-part="lost"]')).toHaveLength(2)
    expect(wrapper.find('[data-part="lost"]').attributes('fill')).toMatch(/^url\(#.+\)$/)
  })

  it('moves the links under the step names when bars are narrow', () => {
    const eight = Array.from({ length: 8 }, (_, i) => step(`s${i}`, 8 - i, 1, (8 - i) / 8))
    const wrapper = render(eight)
    expect(wrapper.findAll('[data-part="chip"]')).toHaveLength(0)
    const links = wrapper.find('ol').findAllComponents(RouterLinkStub)
    expect(links).toHaveLength(7)
    expect(links[0]!.text()).toBe('1 dropped off')
    expect(links[0]!.props('to')).toBe('/dropped/2')
  })

  it('says when nobody entered', () => {
    const wrapper = render([step('visit', 0, 1, 0), step('signup', 0, 0, 0)])
    expect(wrapper.text()).toContain('No one entered this funnel in the range')
    expect(wrapper.findAll('[data-part="chip"]')).toHaveLength(0)
    expect(wrapper.findAll('[data-part="kept"]')).toHaveLength(0)
    expect(wrapper.findAllComponents(RouterLinkStub)).toHaveLength(0)
  })

  it('lists steps with their people for screen readers', () => {
    const wrapper = render(three)
    const items = wrapper.findAll('ol[aria-label="Funnel steps"] li')
    expect(items).toHaveLength(3)
    expect(items[1]!.text()).toContain('2 people')
    expect(items[1]!.text()).toContain('66.7% from step 1')
    expect(wrapper.find('figcaption').text()).toBe(
      'Funnel of 3 steps: 3 people entered, 1 finished, 33.3%',
    )
  })
})
