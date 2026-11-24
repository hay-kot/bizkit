import { describe, expect, it } from 'vitest'

import { hiddenTiles, MAX_TILES, switchesDashboard, tileRoom } from './dashboardEdit'

describe('dashboard editing', () => {
  it('counts hidden tiles against the cap', () => {
    const saved = [
      { insight_id: 'a', width: 6 },
      { insight_id: 'stale', width: 12 },
    ]
    const hidden = hiddenTiles(saved, new Set(['a']))
    expect(hidden).toEqual([{ insight_id: 'stale', width: 12 }])
    expect(tileRoom(1, hidden.length)).toBe(MAX_TILES - 2)
    expect(tileRoom(MAX_TILES - 1, 1)).toBe(0)
    expect(tileRoom(MAX_TILES, 1)).toBe(0)
  })

  it('asks to discard only when the dashboard changes, not the range', () => {
    const on = (dashboardId: string) => ({ params: { dashboardId } })
    expect(switchesDashboard(on('b'), on('a'))).toBe(true)
    expect(switchesDashboard(on('a'), on('a'))).toBe(false)
  })
})
