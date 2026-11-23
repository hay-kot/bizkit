import { describe, expect, it } from 'vitest'

import { lineDiff, parseCases } from './testCases'

describe('parseCases', () => {
  it('takes named cases with an input and an optional expect', () => {
    expect(
      parseCases([
        { name: 'a', input: { x: 1 } },
        { name: 'b', input: null, expect: { action: 'accept' } },
      ]),
    ).toEqual({
      ok: true,
      cases: [
        { name: 'a', input: { x: 1 } },
        { name: 'b', input: null, expect: { action: 'accept' } },
      ],
    })
  })

  it.each([
    [{}, 'must be a JSON array'],
    [[1], 'Case 1 must be an object'],
    [[{ input: {} }], 'Case 1 needs a "name"'],
    [[{ name: 'a' }], 'Case 1 needs an "input"'],
    [[{ name: 'a', input: {}, extra: 1 }], 'unknown key "extra"'],
    [
      [
        { name: 'a', input: {} },
        { name: ' a ', input: {} },
      ],
      '"a" is used twice',
    ],
  ])('refuses %j', (value, message) => {
    const result = parseCases(value)
    expect(result.ok).toBe(false)
    if (!result.ok) expect(result.error).toContain(message)
  })
})

describe('lineDiff', () => {
  it('marks removed and added lines around common ones', () => {
    expect(lineDiff('{\n  "a": 1\n}', '{\n  "a": 2\n}')).toEqual([
      { kind: 'same', text: '{' },
      { kind: 'removed', text: '  "a": 1' },
      { kind: 'added', text: '  "a": 2' },
      { kind: 'same', text: '}' },
    ])
  })
})
