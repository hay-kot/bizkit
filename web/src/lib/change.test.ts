import { describe, expect, it } from 'vitest'

import { changeOf } from './change'

describe('changeOf', () => {
  it.each([
    [1284, 1000, 'up', '28.4%', 'up 28.4% from the previous period'],
    [97, 100, 'down', '3%', 'down 3% from the previous period'],
    [5, 5, 'flat', '0%', 'no change from the previous period'],
    [5, 0, 'new', 'new', 'new since the previous period'],
    [0, 0, 'flat', '0%', 'no change from the previous period'],
  ])('%d against %d is %s %s', (value, previous, direction, text, spoken) => {
    expect(changeOf(value, previous)).toEqual({ direction, text, spoken })
  })

  it('gives shares in percentage points', () => {
    expect(changeOf(0.25, 0.2, 'points')).toEqual({
      direction: 'up',
      text: '5 pts',
      spoken: 'up 5 percentage points from the previous period',
    })
    expect(changeOf(0.1, 0.125, 'points').text).toBe('2.5 pts')
    expect(changeOf(0.1, 0.125, 'points').direction).toBe('down')
    expect(changeOf(0.3, 0, 'points').direction).toBe('up')
    expect(changeOf(0.2, 0.2001, 'points').direction).toBe('flat')
  })
})
