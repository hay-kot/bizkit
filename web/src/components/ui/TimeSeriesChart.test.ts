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

  it('draws smooth lines', () => {
    const wrapper = mount(TimeSeriesChart, { props: { points, series } })
    const d = wrapper.get('path.stroke-chart-1').attributes('d') ?? ''
    expect(d).toMatch(/^M[\d.]+,[\d.]+ C/)
    expect(d).not.toContain('L')
  })

  it('dots the line from the point before the unfinished bucket', () => {
    const wrapper = mount(TimeSeriesChart, { props: { points, series, partialFrom: 2 } })
    const parts = wrapper
      .findAll('path.stroke-chart-1')
      .map((p) => [p.attributes('data-part'), p.attributes('stroke-dasharray')])
    expect(parts).toEqual([
      ['solid', undefined],
      ['partial', '1 4'],
    ])
    const muted = wrapper
      .findAll('path.stroke-base-content\\/40')
      .map((p) => p.attributes('stroke-dasharray'))
    // The previous period has ended, so it stays dashed to the end.
    expect(muted).toEqual(['4 4'])
  })

  it('dots the whole line when the first bucket is unfinished', () => {
    const wrapper = mount(TimeSeriesChart, { props: { points, series, partialFrom: 0 } })
    expect(wrapper.findAll('path.stroke-chart-1').map((p) => p.attributes('data-part'))).toEqual([
      'partial',
    ])
  })

  it('draws a lone point as a dot', () => {
    const wrapper = mount(TimeSeriesChart, {
      props: { points: points.slice(0, 1), series: series.slice(0, 1) },
    })
    expect(wrapper.find('path.stroke-chart-1').exists()).toBe(false)
    expect(wrapper.find('circle.fill-chart-1').exists()).toBe(true)
  })

  it('draws bars with value badges and a hatched unfinished bar', () => {
    const wrapper = mount(TimeSeriesChart, {
      props: {
        points,
        series: [{ name: 'Share', tone: 'primary' as const, unit: '%' }],
        kind: 'bar',
        partialFrom: 2,
      },
    })
    const bars = wrapper.findAll('svg path[d^="M"]')
    expect(bars).toHaveLength(3)
    expect(bars.slice(0, 2).every((b) => b.classes('fill-chart-1'))).toBe(true)
    const patternId = wrapper.get('pattern').attributes('id')
    expect(bars[2]?.attributes('fill')).toBe(`url(#${patternId})`)
    expect(wrapper.findAll('foreignObject span').map((b) => b.text())).toEqual([
      '10%',
      '40%',
      '25%',
    ])
    expect(wrapper.find('path[fill="none"]').exists()).toBe(false)
  })

  it('leaves badges off bars too narrow for them', () => {
    const many = Array.from({ length: 40 }, (_, i) => ({
      t: String(i),
      label: String(i),
      values: [i + 1],
    }))
    const wrapper = mount(TimeSeriesChart, {
      props: { points: many, series: series.slice(0, 1), kind: 'bar' },
    })
    expect(wrapper.findAll('svg path[d^="M"]')).toHaveLength(40)
    expect(wrapper.find('foreignObject').exists()).toBe(false)
  })

  it('writes the unit after values', async () => {
    const wrapper = mount(TimeSeriesChart, {
      props: { points, series: [{ name: 'Share', tone: 'primary' as const, unit: '%' }] },
    })
    await wrapper.get('svg').trigger('focus')
    expect(wrapper.get('[aria-live="polite"]').text()).toBe('Mar 15: Share 25%')
  })

  it('shows a flat baseline and a message without data', () => {
    const wrapper = mount(TimeSeriesChart, { props: { points: [], series } })
    expect(wrapper.text()).toContain('No data for this range')
    expect(wrapper.findAll('line')).toHaveLength(1)
    expect(wrapper.find('path').exists()).toBe(false)
    expect(wrapper.get('svg').attributes('tabindex')).toBeUndefined()
  })
})
