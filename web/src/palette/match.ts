import type { PaletteRow } from './types'

// The tiers are sized so a lower one never outweighs the one above it.
const PREFIX_BONUS = 500
const WORD_START_BONUS = 40
const CONSECUTIVE_BONUS = 8
const GAP_PENALTY = 1

const TITLE_BAND = 1_000_000
const KEYWORD_BAND = 500_000
const GROUP_BAND = 100_000

export interface Match {
  score: number
  /** Indexes in `text` of the matched characters. */
  positions: number[]
}

function isWordStart(chars: string[], i: number): boolean {
  return i === 0 || !/[\p{L}\p{N}]/u.test(chars[i - 1] ?? '')
}

/**
 * Positions count code points, so highlight with `[...text]`.
 *
 * Matches `query` as a case-insensitive subsequence of `text` and scores the
 * best alignment: word starts and runs score, gaps cost. Text that starts
 * with the query scores in a tier above any scattered match. Undefined when
 * the query isn't a subsequence.
 */
export function fuzzyMatch(query: string, text: string): Match | undefined {
  const q = [...query.toLowerCase()]
  const chars = [...text]
  const t = chars.map((c) => c.toLowerCase())
  if (q.length === 0) return { score: 0, positions: [] }
  if (q.length > t.length) return undefined
  const charScore = (j: number) => 1 + (isWordStart(chars, j) ? WORD_START_BONUS : 0)
  // best[i][j]: the best score with q[i] matched at t[j].
  const best: number[][] = []
  const from: number[][] = []
  for (let i = 0; i < q.length; i++) {
    best.push(new Array<number>(t.length).fill(-Infinity))
    from.push(new Array<number>(t.length).fill(-1))
    for (let j = i; j < t.length; j++) {
      if (t[j] !== q[i]) continue
      if (i === 0) {
        best[i]![j] = charScore(j) - j * GAP_PENALTY
        continue
      }
      for (let k = i - 1; k < j; k++) {
        const prev = best[i - 1]![k]!
        if (prev === -Infinity) continue
        const step = k === j - 1 ? CONSECUTIVE_BONUS : -(j - k - 1) * GAP_PENALTY
        const score = prev + step + charScore(j)
        if (score > best[i]![j]!) {
          best[i]![j] = score
          from[i]![j] = k
        }
      }
    }
  }
  const last = q.length - 1
  let end = -1
  for (let j = 0; j < t.length; j++) {
    if (best[last]![j]! > (end === -1 ? -Infinity : best[last]![end]!)) end = j
  }
  if (end === -1) return undefined
  const positions: number[] = []
  for (let i = last, j = end; i >= 0; j = from[i]![j]!, i--) positions.unshift(j)
  let score = best[last]![end]!
  if (t.slice(0, q.length).join('') === q.join('')) {
    let prefix = PREFIX_BONUS + (q.length - 1) * CONSECUTIVE_BONUS
    for (let j = 0; j < q.length; j++) prefix += charScore(j)
    if (prefix > score) {
      score = prefix
      positions.splice(0, positions.length, ...q.map((_, j) => j))
    }
  }
  return { score, positions }
}

/** The terms of a query: words split on spaces and path separators. */
export function terms(query: string): string[] {
  return query
    .trim()
    .split(/[\s>/:,]+/)
    .filter((term) => term.length > 0)
}

/**
 * How well a row matches a query; -1 when it doesn't. The whole query in
 * the title ranks first; otherwise every term must match the title, a
 * keyword or the group (in that order of weight), and with several terms
 * each must start a word.
 */
export function scoreRow(row: PaletteRow, query: string): number {
  const trimmed = query.trim()
  if (trimmed === '') return 0
  const whole = fuzzyMatch(trimmed, row.title)
  if (whole) return TITLE_BAND + whole.score
  const words = terms(trimmed)
  const several = words.length > 1
  let band = 0
  let total = 0
  for (const term of words) {
    const fits = (text: string) => {
      const match = fuzzyMatch(term, text)
      if (!match) return undefined
      if (several && !isWordStart([...text], match.positions[0] ?? 0)) return undefined
      return match
    }
    const inTitle = fits(row.title)
    const inKeyword = (row.keywords ?? [])
      .map(fits)
      .filter((m): m is Match => m !== undefined)
      .sort((a, b) => b.score - a.score)[0]
    const inGroup = row.group === undefined ? undefined : fits(row.group)
    const hit = inTitle
      ? [TITLE_BAND, inTitle]
      : inKeyword
        ? [KEYWORD_BAND, inKeyword]
        : inGroup
          ? [GROUP_BAND, inGroup]
          : undefined
    if (!hit) return -1
    const [termBand, match] = hit as [number, Match]
    band = Math.max(band, termBand)
    total += match.score
  }
  return band + total
}

/** The title characters a query matched, for highlighting. */
export function titleMatches(row: PaletteRow, query: string): Set<number> {
  const trimmed = query.trim()
  const whole = fuzzyMatch(trimmed, row.title)
  if (whole) return new Set(whole.positions)
  const hits = new Set<number>()
  for (const term of terms(trimmed)) {
    for (const i of fuzzyMatch(term, row.title)?.positions ?? []) hits.add(i)
  }
  return hits
}

/** Where each group sits: its rows' lowest `order`, then its name. */
function groupRanks(rows: PaletteRow[]): Map<string, number> {
  const lowest = new Map<string, number>()
  for (const row of rows) {
    const group = row.group ?? ''
    lowest.set(group, Math.min(lowest.get(group) ?? Infinity, row.order ?? 0))
  }
  const sorted = [...lowest.entries()].sort((a, b) => a[1] - b[1] || a[0].localeCompare(b[0]))
  return new Map(sorted.map(([group], index) => [group, index]))
}

/**
 * Rows in display order: by score with a query, then by group placement.
 * Registration order holds within a group.
 */
export function rank(rows: PaletteRow[], query: string): PaletteRow[] {
  const ranks = groupRanks(rows)
  return rows
    .map((row, index) => ({ row, index, score: scoreRow(row, query) }))
    .filter((entry) => entry.score >= 0)
    .sort(
      (a, b) =>
        b.score - a.score ||
        (ranks.get(a.row.group ?? '') ?? 0) - (ranks.get(b.row.group ?? '') ?? 0) ||
        a.index - b.index,
    )
    .map((entry) => entry.row)
}
