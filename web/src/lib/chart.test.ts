import { describe, expect, it } from 'vitest'

import { niceTicks, spreadIndexes } from './chart'

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
