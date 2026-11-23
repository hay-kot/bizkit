import { describe, expect, it } from 'vitest'
import { effectScope, ref } from 'vue'

import { recents, recordRun, reloadRecents, RECENT_LIMIT } from './recents'
import { paletteRows, usePaletteRows } from './registry'

describe('usePaletteRows', () => {
  it('adds rows for the life of the scope and follows their data', () => {
    const names = ref(['Contact'])
    const scope = effectScope()
    scope.run(() =>
      usePaletteRows(() => names.value.map((title) => ({ id: `form:${title}`, title }))),
    )
    expect(paletteRows.value.map((r) => r.title)).toEqual(['Contact'])
    names.value = ['Contact', 'Newsletter']
    expect(paletteRows.value.map((r) => r.title)).toEqual(['Contact', 'Newsletter'])
    scope.stop()
    expect(paletteRows.value).toEqual([])
  })

  it('keeps the first row for an id', () => {
    const scope = effectScope()
    scope.run(() => {
      usePaletteRows([{ id: 'a', title: 'First' }])
      usePaletteRows([{ id: 'a', title: 'Second' }])
    })
    expect(paletteRows.value.map((r) => r.title)).toEqual(['First'])
    scope.stop()
  })
})

describe('recents', () => {
  it('records newest first, once, up to the limit', () => {
    recents.value = []
    for (let i = 0; i < RECENT_LIMIT + 2; i++) recordRun(`row${i}`)
    recordRun('row3')
    expect(recents.value[0]).toBe('row3')
    expect(recents.value).toHaveLength(RECENT_LIMIT)
    expect(new Set(recents.value).size).toBe(RECENT_LIMIT)
  })

  it('ignores stored data of the wrong shape', () => {
    localStorage.setItem('bizkit.palette.recents', '{"not":"a list"}')
    reloadRecents()
    expect(recents.value).toEqual([])
    localStorage.setItem('bizkit.palette.recents', '["a", 3, "b"]')
    reloadRecents()
    expect(recents.value).toEqual(['a', 'b'])
  })
})
