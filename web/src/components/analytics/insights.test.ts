import { describe, expect, it } from 'vitest'

import {
  breakdownOfChoice,
  describeQuery,
  draftFromLink,
  draftOf,
  emptyInsight,
  isRunnable,
  linkFromExplorer,
  opsForChoice,
  queryOf,
} from './insightDraft'
import { displayOptions, insightView, resolveDisplay, storedDisplay } from './insightView'

describe('insight drafts', () => {
  it('need an event for the events kind', () => {
    const draft = emptyInsight()
    expect(isRunnable(draft)).toBe(false)
    expect(isRunnable({ ...draft, kind: 'people' })).toBe(true)
    expect(isRunnable({ ...draft, event: 'signup' })).toBe(true)
  })

  it('send numbers for gt and lt, text for is as typed, and drop empty filters', () => {
    const draft = {
      ...emptyInsight(),
      event: ' signup ',
      filters: [
        { key: 'seats', op: 'gt' as const, value: ' 3 ' },
        { key: 'plan', op: 'is' as const, value: '2' },
        { key: 'tier', op: 'is_not' as const, value: ' pro ' },
        { key: 'beta', op: 'is_set' as const, value: 'ignored' },
        { key: ' ', op: 'is' as const, value: 'x' },
      ],
    }
    expect(queryOf(draft)).toEqual({
      kind: 'events',
      event: ' signup ',
      measure: 'total',
      breakdown: null,
      filters: [
        { key: 'seats', op: 'gt', value: 3 },
        { key: 'plan', op: 'is', value: '2' },
        { key: 'tier', op: 'is_not', value: ' pro ' },
        { key: 'beta', op: 'is_set' },
      ],
    })
  })

  it('leave out filters that are still being filled in', () => {
    const draft = {
      ...emptyInsight(),
      event: 'e',
      filters: [
        { key: 'seats', op: 'gt' as const, value: '' },
        { key: 'seats', op: 'lt' as const, value: 'many' },
        { key: 'plan', op: 'is' as const, value: ' ' },
        { key: 'plan', op: 'is_not' as const, value: 'free' },
      ],
    }
    expect(queryOf(draft).filters).toEqual([{ key: 'plan', op: 'is_not', value: 'free' }])
  })

  it('leave the breakdown out of a per-user distribution, and use traits for people', () => {
    const base = {
      ...emptyInsight(),
      event: 'e',
      breakdownSource: 'property' as const,
      breakdownKey: 'plan',
    }
    expect(queryOf({ ...base, measure: 'per_user' }).breakdown).toBeNull()
    expect(queryOf({ ...base, kind: 'people' })).toEqual({
      kind: 'people',
      filters: [],
      breakdown: { source: 'trait', key: 'plan' },
    })
  })

  it('round-trip a saved query', () => {
    const query = {
      kind: 'events' as const,
      event: 'workspace_created',
      measure: 'users' as const,
      filters: [{ key: 'n', op: 'gt' as const, value: 2 }],
      breakdown: { source: 'trait' as const, key: 'plan' },
    }
    expect(queryOf(draftOf(query))).toEqual(query)
  })

  it('carry the explorer filter over as is filters', () => {
    const link = linkFromExplorer('signup', ['plan=pro', 'ref=a=b'])
    expect(link).toEqual({ event: 'signup', filter: ['plan=pro', 'ref=a=b'] })
    const draft = draftFromLink(link)
    expect(draft.event).toBe('signup')
    expect(draft.filters).toEqual([
      { key: 'plan', op: 'is', value: 'pro' },
      { key: 'ref', op: 'is', value: 'a=b' },
    ])
    expect(draftFromLink({ filter: 'bad' }).filters).toEqual([])
  })

  it('describe what a query counts', () => {
    expect(describeQuery({ kind: 'people', filters: [{ key: 'a', op: 'is_set' }] })).toBe(
      'People · 1 filter',
    )
    expect(
      describeQuery({
        kind: 'events',
        event: 'signup',
        measure: 'share',
        breakdown: { source: 'property', key: 'plan' },
      }),
    ).toBe('Share of active users · signup · by plan')
  })
})

const NOW = Date.parse('2026-01-10T12:00:00Z')

describe('insight views', () => {
  it('show a share as a percent over time', () => {
    const view = insightView(
      {
        view: 'series',
        measure: 'share',
        interval: 'day',
        value: 0.25,
        active: 40,
        points: [{ start: '2026-01-01T00:00:00Z', value: 0.5 }],
      },
      NOW,
    )
    expect(view.headline).toEqual({ label: 'Share of active users', value: '25%' })
    expect(view.caption).toBe('of 40 active users')
    expect(view.chart?.points[0]?.values).toEqual([50])
  })

  it('list breakdown rows, with not set and the rest last', () => {
    const view = insightView(
      {
        view: 'breakdown',
        measure: 'total',
        value: 12,
        rows: [
          { value: 'pro', result: 6 },
          { value: null, result: 2 },
        ],
        other: { value: null, result: 4 },
      },
      NOW,
    )
    expect(view.bars?.items.map((item) => [item.label, item.value])).toEqual([
      ['pro', 6],
      ['(not set)', 2],
      ['Other', 4],
    ])
  })

  it('label the last per-user bucket as and more', () => {
    const view = insightView(
      {
        view: 'distribution',
        users: 3,
        buckets: [
          { times: 1, or_more: false, users: 2 },
          { times: 5, or_more: true, users: 1 },
        ],
      },
      NOW,
    )
    expect(view.bars?.items.map((item) => item.label)).toEqual(['1 time', '5+ times'])
  })

  it('stop at now and mark the open bucket', () => {
    const points = [
      { start: '2026-01-09T00:00:00Z', value: 3 },
      { start: '2026-01-10T00:00:00Z', value: 1 },
      { start: '2026-01-11T00:00:00Z', value: 0 },
    ]
    const series = {
      view: 'series' as const,
      measure: 'total' as const,
      interval: 'day' as const,
      value: 4,
      points,
    }
    const view = insightView(series, NOW)
    expect(view.chart?.points.map((p) => p.values[0])).toEqual([3, 1])
    expect(view.chart?.partialFrom).toBe(1)
    expect(view.trendValue).toBe(4)
    expect(view.trendMode).toBe('relative')
    // Just after midnight the new day is the open one.
    const next = insightView(series, Date.parse('2026-01-11T00:00:01Z'))
    expect(next.chart?.points).toHaveLength(3)
    expect(next.chart?.partialFrom).toBe(2)
  })

  it('mark the first hour open just after midnight', () => {
    const view = insightView(
      {
        view: 'series',
        measure: 'users',
        interval: 'hour',
        value: 1,
        points: [
          { start: '2026-01-10T00:00:00Z', value: 1 },
          { start: '2026-01-10T01:00:00Z', value: 0 },
        ],
      },
      Date.parse('2026-01-10T00:20:00Z'),
    )
    expect(view.chart?.points).toHaveLength(1)
    expect(view.chart?.partialFrom).toBe(0)
  })

  it('leave a past range with nothing open', () => {
    const view = insightView(
      {
        view: 'series',
        measure: 'total',
        interval: 'day',
        value: 1,
        points: [{ start: '2026-01-01T00:00:00Z', value: 1 }],
      },
      NOW,
    )
    expect(view.chart?.partialFrom).toBeUndefined()
  })

  it('compare shares in points', () => {
    const view = insightView(
      { view: 'series', measure: 'share', interval: 'day', value: 0.3, active: 10, points: [] },
      NOW,
    )
    expect(view.trendMode).toBe('points')
    expect(view.chart?.series[0]?.unit).toBe('%')
  })

  it('count people against everyone known', () => {
    const view = insightView({ view: 'count', people: 137, known: 1204 }, NOW)
    expect(view.headline.value).toBe('137')
    expect(view.caption).toBe('of 1,204 known')
    expect(view.trendValue).toBeUndefined()
  })
})

describe('insight displays', () => {
  it('offer what each view can draw, default first', () => {
    expect(displayOptions('series')).toEqual(['line', 'bar', 'number'])
    expect(displayOptions('breakdown')).toEqual(['list', 'number'])
    expect(displayOptions('count')).toEqual(['number'])
  })

  it('draw the saved choice when the view can, else the default', () => {
    expect(resolveDisplay('auto', 'series')).toBe('line')
    expect(resolveDisplay('bar', 'series')).toBe('bar')
    expect(resolveDisplay('number', 'breakdown')).toBe('number')
    expect(resolveDisplay('bar', 'breakdown')).toBe('list')
    expect(resolveDisplay('line', 'distribution')).toBe('list')
    expect(resolveDisplay('line', 'count')).toBe('number')
  })

  it('store a ranked list as bar', () => {
    expect(storedDisplay('list')).toBe('bar')
    expect(storedDisplay('number')).toBe('number')
  })
})

describe('filter comparisons', () => {
  const values = (ops: { value: string }[]) => ops.map((o) => o.value)

  it('offer gt and lt for new choices only when the key has numbers', () => {
    expect(values(opsForChoice(['string'], ['is']))).toEqual(['is', 'is_not', 'is_set'])
    expect(values(opsForChoice(['string', 'number'], ['is']))).toContain('gt')
    expect(values(opsForChoice(undefined, ['is']))).toContain('lt')
  })

  it('keep the saved op and the current choice even when the key types rule them out', () => {
    expect(values(opsForChoice(['string'], ['gt', 'is']))).toEqual(['is', 'is_not', 'gt', 'is_set'])
    expect(values(opsForChoice(['boolean'], ['is', 'lt']))).toContain('lt')
  })
})

describe('breakdown choices', () => {
  it('read a listed choice with its source', () => {
    expect(breakdownOfChoice('trait:plan', 'events')).toEqual({
      breakdownSource: 'trait',
      breakdownKey: 'plan',
    })
    expect(breakdownOfChoice('property:a:b', 'events')).toEqual({
      breakdownSource: 'property',
      breakdownKey: 'a:b',
    })
  })

  it("map a key typed by hand to the kind's source", () => {
    expect(breakdownOfChoice('utm_source', 'events')).toEqual({
      breakdownSource: 'property',
      breakdownKey: 'utm_source',
    })
    expect(breakdownOfChoice('team:size', 'people')).toEqual({
      breakdownSource: 'trait',
      breakdownKey: 'team:size',
    })
  })

  it('clear the breakdown for None', () => {
    expect(breakdownOfChoice('', 'events')).toEqual({ breakdownSource: '', breakdownKey: '' })
  })
})
