import { describe, expect, it } from 'vitest'

import { formatDate, isPast } from './format'

describe('format', () => {
  it('formats dates and falls back when empty', () => {
    expect(formatDate('2026-11-22T12:00:00Z')).toMatch(/2026/)
    expect(formatDate(null, 'Never')).toBe('Never')
  })

  it('knows the past', () => {
    expect(isPast('2000-01-01T00:00:00Z')).toBe(true)
    expect(isPast('2999-01-01T00:00:00Z')).toBe(false)
  })
})
