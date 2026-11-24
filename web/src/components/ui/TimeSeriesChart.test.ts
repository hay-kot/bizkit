import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import TimeSeriesChart from './TimeSeriesChart.vue'

const series = [
  { name: 'Visitors', tone: 'primary' as const },
  { name: 'Previous', tone: 'muted' as const },
]
const points = [
  { t: '2026-03-13', label: 'Mar 13', values: [10, 8] },
  { t: '2026-03-14', label: 'Mar 14', values: [40, 12] },
  { t: '2026-03-15', label: 'Mar 15', values: [25, 30] },
]

describe('TimeSeriesChart', () => {
  it('draws a line per series, an area for the first, and round y ticks', () => {
    const wrapper = mount(TimeSeriesChart, { props: { points, series } })
    const svg = wrapper.get('svg[role="img"]')
    expect(svg.attributes('aria-label')).toBe(
      'Visitors and Previous from Mar 13 to Mar 15. Visitors total 75, Previous total 50.',
    )
    expect(wrapper.findAll('path.stroke-chart-1')).toHaveLength(1)
    const muted = wrapper.get('path.stroke-base-content\\/40')
    expect(muted.attributes('stroke-dasharray')).toBe('4 4')
    expect(wrapper.findAll('path.fill-chart-1\\/10')).toHaveLength(1)
    const yLabels = wrapper.findAll('g > g > text').map((t) => t.text())
    expect(yLabels).toEqual(['0', '20', '40'])
  })

  it('gives each solid series the next chart color, skipping muted ones', () => {
    const wrapper = mount(TimeSeriesChart, {
      props: {
        points: points.map((p) => ({ ...p, values: [...p.values, 1] })),
        series: [
          { name: 'A', tone: 'primary' as const },
          { name: 'Previous', tone: 'muted' as const },
          { name: 'B', tone: 'primary' as const },
        ],
      },
    })
    const lines = wrapper.findAll('path[fill="none"]').map((p) => p.classes())
    expect(lines[0]).toContain('stroke-chart-1')
    expect(lines[1]).toContain('stroke-base-content/40')
    expect(lines[2]).toContain('stroke-chart-2')
  })

  it('has a hidden data table', () => {
    const wrapper = mount(TimeSeriesChart, { props: { points, series } })
    // sr-only must sit on a wrapper: on the table itself it keeps the
    // table's full height and stretches the page.
    expect(wrapper.find('table.sr-only').exists()).toBe(false)
    const rows = wrapper
      .findAll('div.sr-only > table tbody tr')
      .map((r) => r.findAll('th, td').map((c) => c.text()))
    expect(rows).toEqual([
      ['Mar 13', '10', '8'],
      ['Mar 14', '40', '12'],
      ['Mar 15', '25', '30'],
    ])
  })

  it('moves between points with the arrow keys', async () => {
    const wrapper = mount(TimeSeriesChart, { props: { points, series } })
    const svg = wrapper.get('svg')
    const live = () => wrapper.get('[aria-live="polite"]').text()
    expect(svg.attributes('tabindex')).toBe('0')
    await svg.trigger('focus')
    expect(live()).toBe('Mar 15: Visitors 25, Previous 30')
    await svg.trigger('keydown', { key: 'ArrowLeft' })
    expect(live()).toBe('Mar 14: Visitors 40, Previous 12')
    expect(wrapper.get('foreignObject').text()).toContain('Mar 14')
    await svg.trigger('keydown', { key: 'Home' })
    await svg.trigger('keydown', { key: 'ArrowLeft' })
    expect(live()).toBe('Mar 13: Visitors 10, Previous 8')
    await svg.trigger('keydown', { key: 'Escape' })
    expect(live()).toBe('')
    expect(wrapper.find('foreignObject').exists()).toBe(false)
  })

  it('shows a flat baseline and a message without data', () => {
    const wrapper = mount(TimeSeriesChart, { props: { points: [], series } })
    expect(wrapper.text()).toContain('No data for this range')
    expect(wrapper.findAll('line')).toHaveLength(1)
    expect(wrapper.find('path').exists()).toBe(false)
    expect(wrapper.get('svg').attributes('tabindex')).toBeUndefined()
  })
})
