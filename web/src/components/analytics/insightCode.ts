import type { InsightQuery } from '@/queries/analytics'

import {
  draftOf,
  emptyInsight,
  type InsightDraft,
  isRunnable,
  MAX_FILTERS,
  MEASURES,
  queryOf,
} from './insightDraft'

export type EditorMode = 'builder' | 'code'

/**
 * What the insight editor holds. In `builder` the draft is the query; in
 * `code` the JSON text is, and the draft is only where the builder picks
 * up again once the text parses into something it can show.
 */
export interface InsightEditorState {
  mode: EditorMode
  draft: InsightDraft
  code: string
}

export type Outcome<T> = { ok: true; value: T } | { ok: false; error: string }

const QUERY_KEYS = new Set(['kind', 'event', 'filters', 'measure', 'breakdown'])
const FILTER_KEYS = new Set(['key', 'op', 'value'])
const BREAKDOWN_KEYS = new Set(['source', 'key'])
const OPS = new Set(['is', 'is_not', 'gt', 'lt', 'is_set'])

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function unknownKey(value: Record<string, unknown>, known: Set<string>): string | undefined {
  return Object.keys(value).find((key) => !known.has(key))
}

/** The query as the Code view shows it. */
export function codeOf(draft: InsightDraft): string {
  return JSON.stringify(queryOf(draft), null, 2)
}

/**
 * The JSON text as a query to run or save. Only the shape is checked here;
 * the server validates the rest and its errors show on the result.
 */
export function parseCode(code: string): Outcome<InsightQuery> {
  let value: unknown
  try {
    value = JSON.parse(code)
  } catch (err) {
    return { ok: false, error: `Invalid JSON: ${err instanceof Error ? err.message : String(err)}` }
  }
  if (!isObject(value) || typeof value.kind !== 'string') {
    return { ok: false, error: 'The query must be a JSON object with a "kind".' }
  }
  return { ok: true, value: value as unknown as InsightQuery }
}

/**
 * The builder's draft for a parsed query, or why the builder can't show
 * it. The builder only switches to a draft that turns back into the same
 * query (compared by `canonicalQuery`), so anything it would drop or change
 * keeps the editor on Code.
 */
export function draftOfValue(value: unknown): Outcome<InsightDraft> {
  const fail = (error: string): Outcome<InsightDraft> => ({ ok: false, error })
  if (!isObject(value)) return fail('The query must be a JSON object.')
  const extra = unknownKey(value, QUERY_KEYS)
  if (extra) return fail(`The builder has no field "${extra}".`)
  const { kind, event, measure, filters = [], breakdown = null } = value
  if (kind !== 'events' && kind !== 'people') return fail('"kind" must be "events" or "people".')
  if (kind === 'people' && event != null) return fail('A people insight has no "event".')
  if (kind === 'people' && measure != null) return fail('A people insight has no "measure".')
  if (event != null && typeof event !== 'string') return fail('"event" must be text.')
  if (measure != null && !MEASURES.some((m) => m.value === measure)) {
    return fail(`"measure" must be one of ${MEASURES.map((m) => m.value).join(', ')}.`)
  }

  if (!Array.isArray(filters)) return fail('"filters" must be a list.')
  if (filters.length > MAX_FILTERS) {
    return fail(`The builder shows at most ${MAX_FILTERS} filters, and this has ${filters.length}.`)
  }
  for (const [n, filter] of filters.entries()) {
    const at = `Filter ${n + 1}`
    if (!isObject(filter)) return fail(`${at} must be an object.`)
    const field = unknownKey(filter, FILTER_KEYS)
    if (field) return fail(`${at} has a field the builder doesn't know: "${field}".`)
    if (typeof filter.key !== 'string' || filter.key.trim() === '') {
      return fail(`${at} needs a "key".`)
    }
    if (typeof filter.op !== 'string' || !OPS.has(filter.op)) {
      return fail(`${at} has an unknown "op".`)
    }
    const v = filter.value
    if (filter.op === 'is_set') {
      if (v != null) return fail(`${at} is "is_set", which takes no value.`)
      continue
    }
    if (typeof v !== 'string' && typeof v !== 'number' && typeof v !== 'boolean') {
      return fail(`${at} needs a text, number or boolean "value".`)
    }
    if ((filter.op === 'gt' || filter.op === 'lt') && !Number.isFinite(Number(v))) {
      return fail(`${at} compares with "${filter.op}", so its value must be a number.`)
    }
    if (String(v).trim() === '') return fail(`${at} needs a "value".`)
  }

  if (breakdown !== null) {
    if (!isObject(breakdown)) return fail('"breakdown" must be an object or null.')
    const field = unknownKey(breakdown, BREAKDOWN_KEYS)
    if (field) return fail(`"breakdown" has a field the builder doesn't know: "${field}".`)
    if (breakdown.source !== 'property' && breakdown.source !== 'trait') {
      return fail('"breakdown.source" must be "property" or "trait".')
    }
    if (typeof breakdown.key !== 'string' || breakdown.key.trim() === '') {
      return fail('"breakdown" needs a "key".')
    }
    if (kind === 'people' && breakdown.source !== 'trait') {
      return fail('A people insight breaks down by trait only.')
    }
    if (measure === 'per_user') return fail('A per-user distribution has no breakdown.')
  }

  const draft = draftOf(value as unknown as InsightQuery)
  if (
    draft.filters.some((f) => f.key !== f.key.trim()) ||
    draft.breakdownKey.trim() !== draft.breakdownKey
  ) {
    return fail('Keys with spaces at the ends only work in Code.')
  }
  if (canonicalQuery(queryOf(draft)) !== canonicalQuery(value)) {
    return fail('The builder would change this query, so it stays in Code.')
  }
  return { ok: true, value: draft }
}

/** Code back to the builder: the draft, or why the editor stays on Code. */
export function draftOfCode(code: string): Outcome<InsightDraft> {
  const parsed = parseCode(code)
  return parsed.ok ? draftOfValue(parsed.value) : parsed
}

/** A fresh editor on the builder. */
export function editorOfDraft(draft: InsightDraft = emptyInsight()): InsightEditorState {
  return { mode: 'builder', draft, code: codeOf(draft) }
}

/**
 * An editor for a saved query: the builder when it can show the query,
 * else Code with the query as saved.
 */
export function editorOfQuery(query: InsightQuery): InsightEditorState {
  const draft = draftOfValue(query)
  if (draft.ok) return editorOfDraft(draft.value)
  return { mode: 'code', draft: emptyInsight(), code: JSON.stringify(query, null, 2) }
}

/**
 * Code to the builder: the next editor state and the notice to show. On
 * success the notice is empty, so an older error goes away.
 */
export function builderOfCode(editor: InsightEditorState): {
  editor: InsightEditorState
  notice: string
} {
  const next = draftOfCode(editor.code)
  if (!next.ok) return { editor, notice: `${next.error} Fix it here, or keep using Code.` }
  return { editor: { ...editor, mode: 'builder', draft: next.value }, notice: '' }
}

/**
 * The Run button: the query to run, or no query and the parse error. On
 * success the notice is empty, so an older error goes away.
 */
export function runOfCode(code: string): { query: InsightQuery | undefined; notice: string } {
  const parsed = parseCode(code)
  return parsed.ok
    ? { query: parsed.value, notice: '' }
    : { query: undefined, notice: parsed.error }
}

/** The query the editor holds now, from the draft or the JSON text. */
export function currentQuery(editor: InsightEditorState): Outcome<InsightQuery> {
  if (editor.mode === 'code') return parseCode(editor.code)
  return { ok: true, value: queryOf(editor.draft) }
}

/** Why the editor's query can't be saved yet, if it can't. */
export function saveBlocker(editor: InsightEditorState): string | undefined {
  if (editor.mode === 'code') {
    return parseCode(editor.code).ok ? undefined : 'Fix the query JSON first.'
  }
  return isRunnable(editor.draft) ? undefined : 'Pick an event first.'
}

function sortedKeys(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortedKeys)
  if (!isObject(value)) return value
  return Object.fromEntries(
    Object.keys(value)
      .filter((key) => value[key] !== undefined)
      .sort()
      .map((key) => [key, sortedKeys(value[key])]),
  )
}

function normalFilter(filter: unknown): unknown {
  if (!isObject(filter)) return filter
  const { value, ...rest } = filter
  if (filter.op === 'is_set' && value == null) return rest
  // The server compares is and is_not values as text, so `true` and
  // "true" ask the same question. This is the one change of a value the
  // builder may make.
  if (
    (filter.op === 'is' || filter.op === 'is_not') &&
    (typeof value === 'number' || typeof value === 'boolean')
  ) {
    return { ...rest, value: String(value) }
  }
  return filter
}

/**
 * A query in one canonical text: defaults filled in and keys sorted, with
 * every value kept exactly as written (no trimming). A saved query and the
 * editor's compare equal only when they ask the same thing.
 */
export function canonicalQuery(query: unknown): string {
  if (!isObject(query)) return JSON.stringify(query)
  const normal: Record<string, unknown> = {
    ...query,
    filters: Array.isArray(query.filters) ? query.filters.map(normalFilter) : (query.filters ?? []),
    breakdown: query.breakdown ?? null,
  }
  if (query.kind === 'events') {
    normal.event = query.event ?? ''
    normal.measure = query.measure ?? 'total'
  } else {
    if (query.event === null) delete normal.event
    if (query.measure === null) delete normal.measure
  }
  return JSON.stringify(sortedKeys(normal))
}
