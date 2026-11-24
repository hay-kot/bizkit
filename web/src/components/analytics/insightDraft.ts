import type {
  BreakdownSource,
  FilterOp,
  InsightFilter,
  InsightKind,
  InsightQuery,
  Measure,
} from '@/queries/analytics'

/** One filter row as the builder edits it: the value is always text. */
export interface FilterDraft {
  key: string
  op: FilterOp
  value: string
}

/** The builder's state. Empty strings stand for "not picked yet". */
export interface InsightDraft {
  kind: InsightKind
  event: string
  filters: FilterDraft[]
  measure: Measure
  breakdownSource: BreakdownSource | ''
  breakdownKey: string
}

/** The most filters a query takes; the server refuses more. */
export const MAX_FILTERS = 10

export const MEASURES: { value: Measure; label: string; description: string }[] = [
  { value: 'total', label: 'Total events', description: 'How many times it happened' },
  { value: 'users', label: 'Unique users', description: 'How many people did it' },
  {
    value: 'share',
    label: 'Share of active users',
    description: 'Of the people with a stored ID and any event in the range, the share who did it',
  },
  {
    value: 'per_user',
    label: 'Times per user',
    description: 'People with a stored ID by how many times they did it',
  },
]

export const FILTER_OPS: { value: FilterOp; label: string; short: string }[] = [
  { value: 'is', label: 'is', short: 'is' },
  { value: 'is_not', label: 'is not', short: 'is not' },
  { value: 'gt', label: 'greater than', short: '>' },
  { value: 'lt', label: 'less than', short: '<' },
  { value: 'is_set', label: 'is set', short: 'is set' },
]

/**
 * The comparisons that fit a key whose values take `types`. `gt` and `lt`
 * never match text, so they only show when the key has numbers; a key the
 * schema doesn't know (`undefined`) gets every comparison.
 */
export function opsFor(types: string[] | undefined): typeof FILTER_OPS {
  if (!types || types.includes('number')) return FILTER_OPS
  return FILTER_OPS.filter((o) => o.value !== 'gt' && o.value !== 'lt')
}

/**
 * The comparisons to offer for a key: those that fit its types, plus the
 * ones in `keep` (the filter's saved op and the current choice). The type
 * check only narrows new choices; it never hides or replaces an op the
 * filter already has.
 */
export function opsForChoice(types: string[] | undefined, keep: FilterOp[]): typeof FILTER_OPS {
  const fit = new Set(opsFor(types).map((o) => o.value))
  return FILTER_OPS.filter((o) => fit.has(o.value) || keep.includes(o.value))
}

/** A filter as a chip reads it: `plan is pro`, `seats > 3`, `beta is set`. */
export function describeFilter(filter: FilterDraft): string {
  const op = FILTER_OPS.find((o) => o.value === filter.op)?.short ?? filter.op
  return filter.op === 'is_set' ? `${filter.key} ${op}` : `${filter.key} ${op} ${filter.value}`
}

export function emptyInsight(): InsightDraft {
  return {
    kind: 'events',
    event: '',
    filters: [],
    measure: 'total',
    breakdownSource: '',
    breakdownKey: '',
  }
}

export function emptyFilter(): FilterDraft {
  return { key: '', op: 'is', value: '' }
}

function filterOf(draft: FilterDraft): InsightFilter {
  const key = draft.key.trim()
  if (draft.op === 'is_set') return { key, op: draft.op }
  // gt and lt compare numbers. is and is_not send the text exactly as
  // typed (a stored " pro " is a different value from "pro"), and the
  // server also compares it as a number when it reads as one.
  if (draft.op === 'is' || draft.op === 'is_not') return { key, op: draft.op, value: draft.value }
  const text = draft.value.trim()
  const number = Number(text)
  const numeric = text !== '' && Number.isFinite(number)
  return { key, op: draft.op, value: numeric ? number : text }
}

/**
 * True when a filter row is complete: a key, and a value when its op takes
 * one (a number for gt and lt). Rows still being filled in are left out of
 * the query, so the result doesn't fail or drop to zero while typing.
 */
export function isComplete(draft: FilterDraft): boolean {
  if (draft.key.trim() === '') return false
  if (draft.op === 'is_set') return true
  const text = draft.value.trim()
  if (text === '') return false
  return (draft.op !== 'gt' && draft.op !== 'lt') || Number.isFinite(Number(text))
}

/** The query the draft asks, ready for `insights.query` or saving. */
export function queryOf(draft: InsightDraft): InsightQuery {
  const filters = draft.filters.filter(isComplete).map(filterOf)
  const key = draft.breakdownKey.trim()
  const breakdown =
    draft.breakdownSource && key && !(draft.kind === 'events' && draft.measure === 'per_user')
      ? { source: draft.kind === 'people' ? ('trait' as const) : draft.breakdownSource, key }
      : null
  if (draft.kind === 'people') return { kind: 'people', filters, breakdown }
  return { kind: 'events', event: draft.event, filters, measure: draft.measure, breakdown }
}

const BREAKDOWN_SOURCES: readonly BreakdownSource[] = ['property', 'trait']

/** The breakdown picker's value for a source and key: `property:plan`. */
export function breakdownChoice(source: BreakdownSource | '', key: string): string {
  return source && key ? `${source}:${key}` : ''
}

/**
 * The breakdown a picker value asks for. A listed value carries its source
 * (`trait:plan`); anything else is a key typed by hand, which breaks down
 * by the kind's own source: an event property for events, a trait for
 * people.
 */
export function breakdownOfChoice(
  value: string,
  kind: InsightKind,
): Pick<InsightDraft, 'breakdownSource' | 'breakdownKey'> {
  if (value.trim() === '') return { breakdownSource: '', breakdownKey: '' }
  const at = value.indexOf(':')
  const source = BREAKDOWN_SOURCES.find((s) => s === value.slice(0, at))
  if (at > 0 && source && value.length > at + 1) {
    return { breakdownSource: source, breakdownKey: value.slice(at + 1) }
  }
  return { breakdownSource: kind === 'people' ? 'trait' : 'property', breakdownKey: value.trim() }
}

/** True when the draft can run: an events insight needs an event name. */
export function isRunnable(draft: InsightDraft): boolean {
  return draft.kind === 'people' || draft.event.trim() !== ''
}

/** A saved query back in the builder. */
export function draftOf(query: InsightQuery): InsightDraft {
  return {
    kind: query.kind,
    event: query.event ?? '',
    filters: (query.filters ?? []).map((f) => ({
      key: f.key,
      op: f.op,
      value: f.value === undefined || f.value === null ? '' : String(f.value),
    })),
    measure: query.measure ?? 'total',
    breakdownSource: query.breakdown?.source ?? '',
    breakdownKey: query.breakdown?.key ?? '',
  }
}

/**
 * A new insight's draft from the URL, as the Events explorer links it:
 * `?event=` and repeated `?filter=key=value` (an `is` filter each).
 */
export function draftFromLink(query: Record<string, unknown>): InsightDraft {
  const draft = emptyInsight()
  if (typeof query.event === 'string') draft.event = query.event
  const raw = query.filter
  const list = Array.isArray(raw) ? raw : raw === undefined ? [] : [raw]
  for (const item of list) {
    if (typeof item !== 'string') continue
    const at = item.indexOf('=')
    if (at < 1) continue
    draft.filters.push({ key: item.slice(0, at), op: 'is', value: item.slice(at + 1) })
  }
  return draft
}

/** The link from the explorer's filter to a new insight that keeps it. */
export function linkFromExplorer(
  name: string | null,
  props: string[],
): Record<string, string | string[]> {
  return { ...(name ? { event: name } : {}), ...(props.length ? { filter: props } : {}) }
}

const MEASURE_WORDS: Record<Measure, string> = {
  total: 'Events',
  users: 'Users',
  share: 'Share of active users',
  per_user: 'Times per user',
}

/** A short line that says what an insight counts, for lists. */
export function describeQuery(query: InsightQuery): string {
  const head =
    query.kind === 'people'
      ? 'People'
      : `${MEASURE_WORDS[query.measure ?? 'total']} · ${query.event ?? ''}`
  const filters = (query.filters ?? []).length
  const parts = [head]
  if (filters) parts.push(filters === 1 ? '1 filter' : `${filters} filters`)
  if (query.breakdown) parts.push(`by ${query.breakdown.key}`)
  return parts.join(' · ')
}
