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

export const MEASURES: { value: Measure; label: string; description: string }[] = [
  { value: 'total', label: 'Events', description: 'How many times it happened' },
  { value: 'users', label: 'Users', description: 'How many people did it' },
  {
    value: 'share',
    label: 'Share of active users',
    description: 'Of the people with a stored ID and any event in the range, the share who did it',
  },
  {
    value: 'per_user',
    label: 'Per-user distribution',
    description: 'People with a stored ID by how many times they did it',
  },
]

export const FILTER_OPS: { value: FilterOp; label: string }[] = [
  { value: 'is', label: 'is' },
  { value: 'is_not', label: 'is not' },
  { value: 'gt', label: '>' },
  { value: 'lt', label: '<' },
  { value: 'is_set', label: 'is set' },
]

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
  const text = draft.value.trim()
  const number = Number(text)
  // gt and lt compare numbers; is and is_not send text, which the server
  // also compares as a number when it reads as one.
  const numeric = (draft.op === 'gt' || draft.op === 'lt') && text !== '' && Number.isFinite(number)
  return { key, op: draft.op, value: numeric ? number : text }
}

/** The query the draft asks, ready for `insights.query` or saving. */
export function queryOf(draft: InsightDraft): InsightQuery {
  const filters = draft.filters.filter((f) => f.key.trim() !== '').map(filterOf)
  const key = draft.breakdownKey.trim()
  const breakdown =
    draft.breakdownSource && key && !(draft.kind === 'events' && draft.measure === 'per_user')
      ? { source: draft.kind === 'people' ? ('trait' as const) : draft.breakdownSource, key }
      : null
  if (draft.kind === 'people') return { kind: 'people', filters, breakdown }
  return { kind: 'events', event: draft.event.trim(), filters, measure: draft.measure, breakdown }
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
  per_user: 'Per user',
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
