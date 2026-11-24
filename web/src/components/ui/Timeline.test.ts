import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { h } from 'vue'

import Timeline from './Timeline.vue'
import TimelineItem from './TimelineItem.vue'

describe('Timeline', () => {
  it('renders an ordered list of numbered entries with their chips', () => {
    const wrapper = mount(Timeline, {
      props: { label: 'Steps' },
      slots: {
        default: () => [
          h(TimelineItem, { number: 1 }, { default: () => h('p', 'Welcome') }),
          h(
            TimelineItem,
            { number: 2 },
            { default: () => h('p', 'Day one'), chip: () => '1 day after the step before' },
          ),
        ],
      },
    })
    const ol = wrapper.find('ol')
    expect(ol.attributes('aria-label')).toBe('Steps')
    const entries = ol.findAll('li')
    expect(entries).toHaveLength(2)
    expect(entries[0]!.find('.bg-primary').text()).toBe('1')
    expect(entries[1]!.find('.bg-primary').text()).toBe('2')
    expect(entries[0]!.text()).not.toContain('after the step before')
    expect(entries[1]!.text()).toContain('1 day after the step before')
    // The marker is decoration: the content and the list order carry it.
    expect(entries[0]!.find('.bg-primary').attributes('aria-hidden')).toBe('true')
  })
})
