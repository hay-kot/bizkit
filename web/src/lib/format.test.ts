import { describe, expect, it } from 'vitest'

import {
  formatCompactNumber,
  formatDate,
  formatDateTime,
  formatNumber,
  formatPercent,
  formatRelative,
  isPast,
} from './format'

describe('format', () => {
  it('formats dates and falls back when empty', () => {
    expect(formatDate('2026-11-22T12:00:00Z')).toMatch(/2026/)
    expect(formatDate(null, 'Never')).toBe('Never')
  })

  it('knows the past', () => {
    expect(isPast('2000-01-01T00:00:00Z')).toBe(true)
    expect(isPast('2999-01-01T00:00:00Z')).toBe(false)
  })

  it('gives recent times relative to now and older ones as dates', () => {
    const now = Date.parse('2026-11-22T12:00:00Z')
    const ago = (seconds: number) => new Date(now - seconds * 1000).toISOString()
    // The test runner's locale is English.
    expect(formatRelative(ago(0), '', now)).toBe('now')
    expect(formatRelative(ago(150), '', now)).toBe('2 minutes ago')
    expect(formatRelative(ago(3 * 3600), '', now)).toBe('3 hours ago')
    expect(formatRelative(ago(86_400), '', now)).toBe('yesterday')
    expect(formatRelative(ago(30 * 86_400), '', now)).toBe(formatDate(ago(30 * 86_400)))
    expect(formatRelative(null, 'Never')).toBe('Never')
  })

  it('formats a full timestamp', () => {
    expect(formatDateTime('2026-11-22T12:00:00Z')).toMatch(/2026/)
  })

  it('formats numbers, compact numbers and ratios', () => {
    expect(formatNumber(1284)).toBe('1,284')
    expect(formatCompactNumber(1284)).toBe('1.3K')
    expect(formatPercent(0.037)).toBe('3.7%')
    expect(formatPercent(1.2)).toBe('120%')
  })
})
