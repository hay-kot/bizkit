import { describe, expect, it } from 'vitest'

import {
  attributesToRows,
  canonicalSegment,
  conditionsToRows,
  describeAttributes,
  describeSegment,
  isEveryone,
  rowsProblem,
  rowsToAttributes,
  rowsToConditions,
  uniqueTags,
} from './attributes'

describe('attribute rows', () => {
  it('round-trips typed values', () => {
    const attributes = { plan: 'pro', seats: 12, trial: false }
    const rows = attributesToRows(attributes)
    expect(rows).toEqual([
      { key: 'plan', kind: 'text', value: 'pro' },
      { key: 'seats', kind: 'number', value: '12' },
      { key: 'trial', kind: 'bool', value: 'false' },
    ])
    expect(rowsToAttributes(rows)).toEqual(attributes)
  })

  it('drops rows without a key or with a number that does not parse', () => {
    expect(
      rowsToAttributes([
        { key: ' ', kind: 'text', value: 'x' },
        { key: 'seats', kind: 'number', value: 'many' },
        { key: 'city', kind: 'text', value: '' },
      ]),
    ).toEqual({ city: '' })
  })

  it('turns rows into conditions and back', () => {
    const conditions = [{ key: 'plan', equals: 'pro' }]
    expect(rowsToConditions(conditionsToRows(conditions))).toEqual(conditions)
  })

  it('describes attributes for a cell', () => {
    expect(describeAttributes({ plan: 'pro', seats: 12 })).toBe('plan: pro · seats: 12')
    expect(describeAttributes(undefined)).toBe('')
  })
})

describe('row problems', () => {
  const row = (key: string, kind: 'text' | 'number' | 'bool', value: string) => ({
    key,
    kind,
    value,
  })

  it('ignores blank rows', () => {
    expect(rowsProblem([row('', 'text', ''), row('', 'bool', 'true')])).toBeUndefined()
  })

  it('reports what a save would drop', () => {
    expect(rowsProblem([row('', 'text', 'pro')])).toMatch(/needs a key/)
    expect(rowsProblem([row('seats', 'number', '')])).toMatch(/seats needs a number/)
    expect(rowsProblem([row('plan', 'text', 'a'), row('plan', 'text', 'b')])).toMatch(
      /more than one row/,
    )
  })

  it('reads a value Vue cast to a number', () => {
    const cast = { key: 'seats', kind: 'number' as const, value: 5 as unknown as string }
    expect(rowsProblem([cast])).toBeUndefined()
    expect(rowsToAttributes([cast])).toEqual({ seats: 5 })
  })
})

describe('segments', () => {
  it('drops repeated tags as the server does', () => {
    expect(uniqueTags([' vip', 'vip ', '', 'beta'])).toEqual(['vip', 'beta'])
    expect(canonicalSegment({ tags: ['vip', 'vip'] })).toBe(canonicalSegment({ tags: ['vip'] }))
  })

  it('treats an empty segment as everyone', () => {
    expect(isEveryone({})).toBe(true)
    expect(isEveryone({ tags: [], attributes: [], joined_after: null })).toBe(true)
    expect(describeSegment({})).toBe('Every subscriber')
  })

  it('compares times by instant, not by spelling', () => {
    expect(canonicalSegment({ joined_after: '2026-11-01T05:00:00Z' })).toBe(
      canonicalSegment({ joined_after: '2026-11-01T05:00:00.000Z' }),
    )
  })

  it('compares conditions whatever their key order', () => {
    const parsed = JSON.parse('{"attributes":[{"equals":"pro","key":"plan"}]}')
    expect(canonicalSegment(parsed)).toBe(
      canonicalSegment({ attributes: [{ key: 'plan', equals: 'pro' }] }),
    )
  })

  it('describes tags and conditions', () => {
    expect(
      describeSegment({ tags: ['vip', 'beta'], attributes: [{ key: 'plan', equals: 'pro' }] }),
    ).toBe('Tagged “vip” and “beta” · plan is pro')
  })
})
