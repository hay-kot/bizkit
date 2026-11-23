import { describe, expect, it } from 'vitest'

import {
  addDays,
  presetRange,
  previousPeriod,
  rangeFromQuery,
  rangeLength,
  rangeToQuery,
  utcToday,
} from './range'

describe('range', () => {
  const today = '2026-03-15'

  it.each([
    ['today', '2026-03-15', '2026-03-15'],
    ['7d', '2026-03-09', '2026-03-15'],
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
})
