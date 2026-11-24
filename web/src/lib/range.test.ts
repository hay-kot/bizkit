import { describe, expect, it } from 'vitest'

import {
  addDays,
  presetRange,
  previousPeriod,
  rangeFromQuery,
  rangeLength,
  rangeToQuery,
  savedPreset,
  shiftRange,
  utcToday,
  zoomOut,
} from './range'

describe('range', () => {
  const today = '2026-03-15'

  it.each([
    ['today', '2026-03-15', '2026-03-15'],
    ['yesterday', '2026-03-14', '2026-03-14'],
    ['7d', '2026-03-09', '2026-03-15'],
    ['14d', '2026-03-02', '2026-03-15'],
    ['30d', '2026-02-14', '2026-03-15'],
    ['90d', '2025-12-16', '2026-03-15'],
    ['month', '2026-03-01', '2026-03-15'],
    ['last-month', '2026-02-01', '2026-02-28'],
    ['custom', '2026-03-15', '2026-03-15'],
  ] as const)('preset %s covers %s to %s', (preset, from, to) => {
    expect(presetRange(preset, today)).toEqual({ from, to })
  })

  it('handles last month across a year and a leap day', () => {
    expect(presetRange('last-month', '2026-01-10')).toEqual({
      from: '2025-12-01',
      to: '2025-12-31',
    })
    expect(presetRange('last-month', '2028-03-01')).toEqual({
      from: '2028-02-01',
      to: '2028-02-29',
    })
  })

  it('counts days and moves across months', () => {
    expect(rangeLength({ from: '2026-03-09', to: '2026-03-15' })).toBe(7)
    expect(rangeLength({ from: today, to: today })).toBe(1)
    expect(addDays('2026-03-01', -1)).toBe('2026-02-28')
  })

  it('gives the previous period of the same length', () => {
    expect(previousPeriod({ from: '2026-03-09', to: '2026-03-15' })).toEqual({
      from: '2026-03-02',
      to: '2026-03-08',
    })
    expect(previousPeriod({ from: today, to: today })).toEqual({
      from: '2026-03-14',
      to: '2026-03-14',
    })
    // This month so far (15 days) compares with the 15 days before it, not all of February.
    expect(previousPeriod(presetRange('month', today))).toEqual({
      from: '2026-02-14',
      to: '2026-02-28',
    })
  })

  it('reads ranges back from the URL query', () => {
    expect(rangeFromQuery({ range: '7d' }, today, '30d')).toEqual({
      preset: '7d',
      from: '2026-03-09',
      to: '2026-03-15',
    })
    const custom = { preset: 'custom' as const, from: '2026-01-02', to: '2026-01-09' }
    expect(rangeFromQuery(rangeToQuery(custom), today, '30d')).toEqual(custom)
    expect(rangeToQuery({ preset: '7d', from: 'x', to: 'y' })).toEqual({
      range: '7d',
      from: undefined,
      to: undefined,
    })
  })

  it.each([
    [{}],
    [{ range: 'weekly' }],
    [{ range: 'custom', from: '2026-01-09', to: '2026-01-02' }],
    [{ range: 'custom', from: 'yesterday', to: '2026-01-02' }],
  ])('falls back on a bad query %o', (query) => {
    expect(rangeFromQuery(query, today, '30d').preset).toBe('30d')
  })

  it('takes today in UTC', () => {
    expect(utcToday(new Date('2026-03-15T23:30:00-05:00'))).toBe('2026-03-16')
  })

  describe('shiftRange', () => {
    it('moves by the range length, back and forward', () => {
      const week = { from: '2026-03-02', to: '2026-03-08' }
      expect(shiftRange(week, -1, today)).toEqual({
        preset: 'custom',
        from: '2026-02-23',
        to: '2026-03-01',
      })
      expect(shiftRange(week, 1, today)).toEqual({
        preset: 'custom',
        from: '2026-03-09',
        to: '2026-03-15',
      })
    })

    it('crosses month and year boundaries by days, not months', () => {
      expect(shiftRange({ from: '2026-01-01', to: '2026-01-31' }, -1, today)).toEqual({
        preset: 'custom',
        from: '2025-12-01',
        to: '2025-12-31',
      })
      // February has 28 days, so a 31-day March shifts back past its first.
      expect(shiftRange({ from: '2026-03-01', to: '2026-03-31' }, -1, '2026-04-30')).toEqual({
        preset: 'custom',
        from: '2026-01-29',
        to: '2026-02-28',
      })
    })

    it('moves one day for a one-day range', () => {
      expect(shiftRange({ from: '2026-01-01', to: '2026-01-01' }, -1, today)).toEqual({
        preset: 'custom',
        from: '2025-12-31',
        to: '2025-12-31',
      })
      expect(shiftRange({ from: '2026-03-14', to: '2026-03-14' }, 1, today)).toEqual({
        preset: 'custom',
        from: today,
        to: today,
      })
    })

    it('stops at today and keeps the length', () => {
      expect(shiftRange({ from: '2026-03-05', to: '2026-03-11' }, 1, today)).toEqual({
        preset: 'custom',
        from: '2026-03-09',
        to: today,
      })
      expect(shiftRange({ from: '2026-03-09', to: today }, 1, today)).toEqual({
        preset: 'custom',
        from: '2026-03-09',
        to: today,
      })
    })
  })

  describe('zoomOut', () => {
    it('doubles the span around its middle', () => {
      expect(zoomOut({ from: '2026-02-08', to: '2026-02-14' }, today)).toEqual({
        preset: 'custom',
        from: '2026-02-04',
        to: '2026-02-17',
      })
    })

    it('widens one day to two, the extra day before it', () => {
      expect(zoomOut({ from: '2026-01-01', to: '2026-01-01' }, today)).toEqual({
        preset: 'custom',
        from: '2025-12-31',
        to: '2026-01-01',
      })
    })

    it('clamps the end to today and keeps the doubled length', () => {
      const zoomed = zoomOut({ from: '2026-03-09', to: today }, today)
      expect(zoomed).toEqual({ preset: 'custom', from: '2026-03-02', to: today })
      expect(rangeLength(zoomed)).toBe(14)
      const nearEnd = zoomOut({ from: '2026-03-11', to: '2026-03-14' }, today)
      expect(nearEnd).toEqual({ preset: 'custom', from: '2026-03-08', to: today })
    })

    it('crosses a year boundary', () => {
      expect(zoomOut({ from: '2025-12-30', to: '2026-01-02' }, today)).toEqual({
        preset: 'custom',
        from: '2025-12-28',
        to: '2026-01-04',
      })
    })
  })

  it('saves only the presets the insights API keeps', () => {
    expect(savedPreset('7d')).toBe('7d')
    expect(savedPreset('14d')).toBeUndefined()
    expect(savedPreset('yesterday')).toBeUndefined()
    expect(savedPreset('custom')).toBeUndefined()
  })
})
