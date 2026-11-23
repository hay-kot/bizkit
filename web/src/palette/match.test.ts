import { describe, expect, it } from 'vitest'

import { fuzzyMatch, rank, scoreRow, terms, titleMatches } from './match'
import type { PaletteRow } from './types'

const row = (title: string, extra: Partial<PaletteRow> = {}): PaletteRow => ({
  id: title,
  title,
  ...extra,
})

describe('fuzzyMatch', () => {
  it('needs the query as a subsequence', () => {
    expect(fuzzyMatch('cnt', 'Contact')).toBeDefined()
    expect(fuzzyMatch('xyz', 'Contact')).toBeUndefined()
    expect(fuzzyMatch('contacts', 'Contact')).toBeUndefined()
  })

  it('prefers word starts over scattered letters', () => {
    const wordStarts = fuzzyMatch('ak', 'API keys')!
    const scattered = fuzzyMatch('ak', 'Takeaway')!
    expect(wordStarts.score).toBeGreaterThan(scattered.score)
    expect(wordStarts.positions).toEqual([0, 4])
  })

  it('puts a prefix in a tier of its own', () => {
    expect(fuzzyMatch('se', 'Settings')!.score).toBeGreaterThan(
      fuzzyMatch('se', 'Some event')!.score,
    )
    expect(fuzzyMatch('set', 'Settings')!.positions).toEqual([0, 1, 2])
  })

  it('finds the best alignment, not the first', () => {
    expect(fuzzyMatch('ps', 'Post to Slack')!.positions).toEqual([0, 8])
  })
})

describe('terms', () => {
  it('splits on spaces and path separators', () => {
    expect(terms('  web > forms/contact ')).toEqual(['web', 'forms', 'contact'])
  })
})

describe('scoreRow', () => {
  it('ranks title over keyword over group', () => {
    const title = scoreRow(row('New form'), 'form')
    const keyword = scoreRow(row('Contact', { keywords: ['form'] }), 'form')
    const group = scoreRow(row('Contact', { group: 'Forms' }), 'form')
    expect(title).toBeGreaterThan(keyword)
    expect(keyword).toBeGreaterThan(group)
    expect(group).toBeGreaterThan(0)
  })

  it('needs every term, each on a word start', () => {
    const contact = row('Contact', { group: 'Website' })
    expect(scoreRow(contact, 'web cont')).toBeGreaterThan(0)
    expect(scoreRow(contact, 'web zzz')).toBe(-1)
    expect(scoreRow(contact, 'ebs ont')).toBe(-1)
  })

  it('is zero for an empty query', () => {
    expect(scoreRow(row('Contact'), '  ')).toBe(0)
  })
})

describe('rank', () => {
  it('keeps groups in place by their lowest order on an empty query', () => {
    const rows = [
      row('Team', { group: 'Workspace', order: 50 }),
      row('Contact', { group: 'Forms', order: 10 }),
      row('Settings', { group: 'Workspace', order: 90 }),
      row('Newsletter', { group: 'Forms', order: 10 }),
    ]
    expect(rank(rows, '').map((r) => r.title)).toEqual([
      'Contact',
      'Newsletter',
      'Team',
      'Settings',
    ])
  })

  it('orders matches by score and drops the rest', () => {
    const rows = [row('Webhooks'), row('Settings'), row('Post to Slack')]
    expect(rank(rows, 'set').map((r) => r.title)).toEqual(['Settings'])
    expect(rank(rows, 's')[0]!.title).toBe('Settings')
  })
})

describe('titleMatches', () => {
  it('unions the positions of each term', () => {
    expect([...titleMatches(row('Post to Slack'), 'sl po')].sort((a, b) => a - b)).toEqual([
      0, 1, 8, 9,
    ])
  })
})
