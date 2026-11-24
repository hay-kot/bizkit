import { describe, expect, it } from 'vitest'

import {
  canonicalQuery,
  codeOf,
  currentQuery,
  draftOfCode,
  editorOfQuery,
  saveBlocker,
} from './insightCode'
import { emptyInsight } from './insightDraft'

describe('insight code', () => {
  it('round-trips a builder query through the JSON', () => {
    const draft = {
      ...emptyInsight(),
      event: 'signup',
      measure: 'users' as const,
      filters: [{ key: 'plan', op: 'is' as const, value: 'pro' }],
      breakdownSource: 'trait' as const,
      breakdownKey: 'country',
    }
    const back = draftOfCode(codeOf(draft))
    expect(back).toEqual({ ok: true, value: draft })
  })

  it('reads saved queries with defaults left out', () => {
    expect(draftOfCode('{"kind": "events", "event": "signup"}')).toEqual({
      ok: true,
      value: { ...emptyInsight(), event: 'signup' },
    })
  })

  it('treats boolean and number is values as the text the server compares', () => {
    const draft = draftOfCode(
      '{"kind": "people", "filters": [{"key": "beta", "op": "is", "value": true}]}',
    )
    expect(draft.ok && draft.value.filters).toEqual([{ key: 'beta', op: 'is', value: 'true' }])
  })

  it.each([
    ['not JSON', '{', /Invalid JSON/],
    ['an array', '[]', /JSON object/],
    ['an unknown field', '{"kind": "events", "event": "a", "limit": 5}', /no field "limit"/],
    [
      'too many filters',
      JSON.stringify({ kind: 'people', filters: Array(11).fill({ key: 'k', op: 'is_set' }) }),
      /at most 10 filters/,
    ],
    [
      'a filter field',
      '{"kind": "people", "filters": [{"key": "k", "op": "is_set", "x": 1}]}',
      /"x"/,
    ],
    [
      'an unknown op',
      '{"kind": "people", "filters": [{"key": "k", "op": "like", "value": "a"}]}',
      /unknown "op"/,
    ],
    [
      'text for gt',
      '{"kind": "people", "filters": [{"key": "k", "op": "gt", "value": "many"}]}',
      /must be a number/,
    ],
    [
      'a value on is_set',
      '{"kind": "people", "filters": [{"key": "k", "op": "is_set", "value": 1}]}',
      /takes no value/,
    ],
    [
      'a property breakdown on people',
      '{"kind": "people", "breakdown": {"source": "property", "key": "a"}}',
      /by trait only/,
    ],
    [
      'a per-user breakdown',
      '{"kind": "events", "event": "a", "measure": "per_user", "breakdown": {"source": "trait", "key": "a"}}',
      /no breakdown/,
    ],
    ['an event on people', '{"kind": "people", "event": "a"}', /no "event"/],
  ])('keeps the editor on Code for %s', (_, code, error) => {
    const result = draftOfCode(code)
    expect(result.ok).toBe(false)
    if (!result.ok) expect(result.error).toMatch(error)
  })

  it('opens a saved query the builder can show in the builder, else in Code', () => {
    expect(editorOfQuery({ kind: 'events', event: 'signup' }).mode).toBe('builder')
    const odd = { kind: 'people', filters: Array(11).fill({ key: 'k', op: 'is_set' }) }
    const editor = editorOfQuery(odd as never)
    expect(editor.mode).toBe('code')
    expect(JSON.parse(editor.code)).toEqual(odd)
  })

  it('saves what the JSON says in Code, and blocks text that is not a query', () => {
    const editor = { mode: 'code' as const, draft: emptyInsight(), code: '{"kind": "people"}' }
    expect(currentQuery(editor)).toEqual({ ok: true, value: { kind: 'people' } })
    expect(saveBlocker(editor)).toBeUndefined()
    expect(saveBlocker({ ...editor, code: '{"kind":' })).toBe('Fix the query JSON first.')
    expect(saveBlocker({ ...editor, mode: 'builder' })).toBe('Pick an event first.')
  })

  it('compares queries by what they ask, not how they are written', () => {
    expect(canonicalQuery({ kind: 'events', event: 'a' })).toBe(
      canonicalQuery({
        kind: 'events',
        event: 'a',
        measure: 'total',
        filters: [],
        breakdown: null,
      }),
    )
    expect(canonicalQuery({ kind: 'events', event: 'a' })).not.toBe(
      canonicalQuery({ kind: 'events', event: 'b' }),
    )
  })
})
