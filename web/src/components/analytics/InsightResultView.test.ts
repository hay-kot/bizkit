import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import type { InsightResult } from '@/queries/analytics'

import InsightResultView from './InsightResultView.vue'

const series: InsightResult = {
  view: 'series',
  measure: 'users',
  interval: 'day',
  value: 101,
  points: [
    { start: '2026-01-01T00:00:00Z', value: 60 },
    { start: '2026-01-02T00:00:00Z', value: 41 },
  ],
}

interface ViewProps {
  result: InsightResult
  compact?: boolean
  display?: 'auto' | 'line' | 'bar' | 'number'
  previous?: number
}

function mountView(props: ViewProps) {
  return mount(InsightResultView, { props: { loading: false, error: null, ...props } })
}

describe('InsightResultView', () => {
  it('draws a series as a line under its headline', () => {
    const wrapper = mountView({ result: series })
    expect(wrapper.text()).toContain('101')
    expect(wrapper.find('path[fill="none"]').exists()).toBe(true)
  })

  it('drops the headline over a chart on a compact tile', () => {
    const wrapper = mountView({ result: series, compact: true, display: 'bar' })
    expect(wrapper.text()).not.toContain('101')
    expect(wrapper.find('path[fill="none"]').exists()).toBe(false)
  })

  it('shows the number with its change from the previous period', () => {
    const wrapper = mountView({ result: series, display: 'number', previous: 125 })
    expect(wrapper.find('svg[role="img"]').exists()).toBe(false)
    const trend = wrapper.get('[data-trend]')
    expect(trend.classes()).toContain('text-error')
    expect(trend.text()).toContain('Down 19.2% from the previous period')
    expect(trend.get('.sr-only').text()).toBe('down 19.2% from the previous period')
  })

  it('says new when the previous period had none', () => {
    const wrapper = mountView({ result: series, display: 'number', previous: 0 })
    expect(wrapper.get('[data-trend]').text()).toContain('New this period')
  })

  it('compares a share in points', () => {
    const share: InsightResult = { ...series, measure: 'share', value: 0.25, active: 40 }
    const wrapper = mountView({ result: share, display: 'number', previous: 0.2 })
    expect(wrapper.get('[data-trend]').text()).toContain('Up 5 pts from the previous period')
  })

  it('shows the number alone without a previous value', () => {
    const wrapper = mountView({ result: series, display: 'number' })
    expect(wrapper.text()).toContain('101')
    expect(wrapper.find('[data-trend]').exists()).toBe(false)
  })

  it('draws a saved line on a breakdown as its list', () => {
    const breakdown: InsightResult = {
      view: 'breakdown',
      measure: 'total',
      value: 3,
      rows: [{ value: 'pro', result: 3 }],
    }
    const wrapper = mountView({ result: breakdown, display: 'line' })
    expect(wrapper.text()).toContain('pro')
    expect(wrapper.find('svg[role="img"]').exists()).toBe(false)
  })
})
