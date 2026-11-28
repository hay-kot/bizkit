import { describe, expect, it } from 'vitest'

import {
  barPath,
  type Coord,
  monotoneSegments,
  niceTicks,
  openFrom,
  pathBetween,
  spreadIndexes,
} from './chart'

describe('niceTicks', () => {
  it.each([
    [0, [0, 1]],
    [3, [0, 1, 2, 3]],
    [7, [0, 5, 10]],
    [100, [0, 50, 100]],
    [1284, [0, 500, 1000, 1500]],
    [0.4, [0, 0.2, 0.4]],
  ])('max %d gives %j', (max, ticks) => {
    expect(niceTicks(max).map((t) => Number(t.toFixed(6)))).toEqual(ticks)
  })
})

describe('spreadIndexes', () => {
  it('keeps every index when there are few', () => {
    expect(spreadIndexes(3)).toEqual([0, 1, 2])
  })

  it('spreads over many, with both ends', () => {
    expect(spreadIndexes(30)).toEqual([0, 7, 15, 22, 29])
  })
})

/** The y of each control point and end in a `C` command. */
function ys(segment: string): number[] {
  return segment
    .slice(1)
    .split(' ')
    .map((pair) => Number(pair.split(',')[1]))
}

describe('monotoneSegments', () => {
  it('has one segment per gap, ending on each point', () => {
    const coords: Coord[] = [
      [0, 100],
      [10, 50],
      [20, 80],
    ]
    const segments = monotoneSegments(coords)
    expect(segments).toHaveLength(2)
    expect(segments[0]).toMatch(/ 10,50$/)
    expect(segments[1]).toMatch(/ 20,80$/)
  })

  it('draws a straight line through two points', () => {
    expect(
      monotoneSegments([
        [0, 0],
        [30, 30],
      ]),
    ).toEqual(['C10,10 20,20 30,30'])
  })

  it('never overshoots a peak or a floor', () => {
    // A spike between zeros: an unclamped spline would dip below the floor (y 100).
    const coords: Coord[] = [
      [0, 100],
      [10, 100],
      [20, 0],
      [30, 100],
      [40, 100],
    ]
    const all = monotoneSegments(coords).flatMap(ys)
    expect(Math.max(...all)).toBeLessThanOrEqual(100)
    expect(Math.min(...all)).toBeGreaterThanOrEqual(0)
  })

  it('has nothing to draw for fewer than two points', () => {
    expect(monotoneSegments([])).toEqual([])
    expect(monotoneSegments([[5, 5]])).toEqual([])
  })
})

describe('pathBetween', () => {
  const coords: Coord[] = [
    [0, 0],
    [10, 10],
    [20, 0],
  ]
  const segments = monotoneSegments(coords)

  it('starts at the first point and takes the segments up to the last', () => {
    expect(pathBetween(coords, segments, 0, 2)).toBe(`M0,0 ${segments[0]} ${segments[1]}`)
    expect(pathBetween(coords, segments, 1, 2)).toBe(`M10,10 ${segments[1]}`)
  })

  it('is a lone move for one point and empty past the end', () => {
    expect(pathBetween(coords, segments, 2, 2)).toBe('M20,0')
    expect(pathBetween(coords, segments, 3, 3)).toBe('')
  })
})

describe('barPath', () => {
  it('rounds the top corners only', () => {
    expect(barPath(10, 20, 30, 40, 3)).toBe('M10,60 V23 Q10,20 13,20 H37 Q40,20 40,23 V60 Z')
  })

  it('keeps the radius inside short and thin bars', () => {
    expect(barPath(0, 0, 4, 1, 3)).toBe('M0,1 V1 Q0,0 1,0 H3 Q4,0 4,1 V1 Z')
  })

  it('draws nothing for an empty bar', () => {
    expect(barPath(0, 10, 20, 0, 3)).toBe('')
  })
})

describe('openFrom', () => {
  const DAY = 86_400_000
  const days = ['2026-11-26T00:00:00Z', '2026-11-27T00:00:00Z', '2026-11-28T00:00:00Z']

  it('finds the bucket that runs at now', () => {
    expect(openFrom(days, DAY, Date.parse('2026-11-28T09:00:00Z'))).toBe(2)
  })

  it('treats a bucket as ended at its last millisecond', () => {
    expect(openFrom(days, DAY, Date.parse('2026-11-29T00:00:00Z'))).toBeUndefined()
    expect(openFrom(days, DAY, Date.parse('2026-11-28T23:59:59.999Z'))).toBe(2)
  })

  it('works per hour and for no buckets', () => {
    const hours = ['2026-11-28T08:00:00Z', '2026-11-28T09:00:00Z']
    expect(openFrom(hours, 3_600_000, Date.parse('2026-11-28T09:30:00Z'))).toBe(1)
    expect(openFrom([], DAY, 0)).toBeUndefined()
  })
})
