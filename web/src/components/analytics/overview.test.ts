import { describe, expect, it } from 'vitest'

import type { Summary } from '@/queries/analytics'

import { overviewChart, overviewTiles } from './overview'

const summary: Summary = {
  interval: 'day',
  totals: { visitors: 30, pageviews: 90, events: 12, conversions: 3 },
  previous: { visitors: 20, pageviews: 60, events: 0, conversions: 3 },
  series: [
    { start: '2026-11-26T00:00:00Z', visitors: 10, pageviews: 30 },
    { start: '2026-11-27T00:00:00Z', visitors: 12, pageviews: 40 },
    { start: '2026-11-28T00:00:00Z', visitors: 8, pageviews: 20 },
    { start: '2026-11-29T00:00:00Z', visitors: 0, pageviews: 0 },
  ],
  previous_series: [
    { start: '2026-11-22T00:00:00Z', visitors: 5, pageviews: 15 },
    { start: '2026-11-23T00:00:00Z', visitors: 7, pageviews: 25 },
    { start: '2026-11-24T00:00:00Z', visitors: 8, pageviews: 20 },
    { start: '2026-11-25T00:00:00Z', visitors: 0, pageviews: 0 },
  ],
}
const NOW = Date.parse('2026-11-28T09:00:00Z')

describe('overviewTiles', () => {
  it('gives sparklines that stop at now and end on the open day', () => {
    const [visitors, pageviews] = overviewTiles(summary, true, 4, NOW)
    expect(visitors).toMatchObject({ value: 30, previous: 20, series: [10, 12, 8], partialFrom: 2 })
    expect(pageviews?.series).toEqual([30, 40, 20])
  })

  it('averages visitors per day of the range, to one decimal', () => {
    const tiles = overviewTiles(summary, true, 4, NOW)
    expect(tiles[2]).toMatchObject({ label: 'Avg daily visitors', value: 7.5, previous: 5 })
    expect(overviewTiles(summary, false, 3, NOW)[2]?.value).toBe(10)
  })

  it('leaves previous values out without compare', () => {
    const tiles = overviewTiles(summary, false, 4, NOW)
    expect(tiles.map((tile) => tile.previous)).toEqual([
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
    ])
  })

  it('shows zeros before the summary loads', () => {
    const tiles = overviewTiles(undefined, true, 7, NOW)
    expect(tiles.map((tile) => tile.value)).toEqual([0, 0, 0, 0, 0])
    expect(tiles[0]?.series).toEqual([])
  })
})

describe('overviewChart', () => {
  it('draws the metric against the previous period', () => {
    const chart = overviewChart(summary, 'pageviews', true, NOW)
    expect(chart.series.map((s) => s.name)).toEqual(['Pageviews', 'Previous pageviews'])
    expect(chart.points.map((p) => p.values)).toEqual([
      [30, 15],
      [40, 25],
      [20, 20],
    ])
    expect(chart.partialFrom).toBe(2)
  })

  it('draws the metric alone without compare', () => {
    const chart = overviewChart(summary, 'visitors', false, NOW)
    expect(chart.series).toEqual([{ name: 'Visitors', tone: 'primary' }])
    expect(chart.points.map((p) => p.values)).toEqual([[10], [12], [8]])
  })

  it('labels hours in UTC', () => {
    const hourly: Summary = {
      ...summary,
      interval: 'hour',
      series: [{ start: '2026-11-28T08:00:00Z', visitors: 1, pageviews: 2 }],
    }
    const chart = overviewChart(hourly, 'visitors', false, NOW)
    expect(chart.points[0]?.label).toBe('08:00 UTC')
    expect(chart.partialFrom).toBeUndefined()
  })
})
