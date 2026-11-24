import { formatDate } from '@/lib/format'
import type { AttributeCondition, Attributes, AttributeValue, Segment } from '@/queries/mailing'

export type AttributeKind = 'text' | 'number' | 'bool'

/** One editable key and value. The value stays text until it is sent. */
export interface AttributeRow {
  key: string
  kind: AttributeKind
  value: string
}

function kindOf(value: AttributeValue): AttributeKind {
  if (typeof value === 'number') return 'number'
  if (typeof value === 'boolean') return 'bool'
  return 'text'
}

function toRow(key: string, value: AttributeValue): AttributeRow {
  return { key, kind: kindOf(value), value: String(value) }
}

/** The row's typed value, or undefined when a number doesn't parse. */
export function rowValue(row: AttributeRow): AttributeValue | undefined {
  if (row.kind === 'bool') return row.value === 'true'
  if (row.kind === 'number') {
    const n = Number(row.value)
    return row.value.trim() === '' || Number.isNaN(n) ? undefined : n
  }
  return row.value
}

export function attributesToRows(attributes: Attributes | undefined): AttributeRow[] {
  return Object.entries(attributes ?? {}).map(([key, value]) => toRow(key, value))
}

/** Rows with a key and a value that parses; the server checks the rest. */
export function rowsToAttributes(rows: AttributeRow[]): Attributes {
  const out: Attributes = {}
  for (const row of rows) {
    const value = rowValue(row)
    if (row.key.trim() !== '' && value !== undefined) out[row.key.trim()] = value
  }
  return out
}

export function conditionsToRows(conditions: AttributeCondition[] | undefined): AttributeRow[] {
  return (conditions ?? []).map((c) => toRow(c.key, c.equals))
}

export function rowsToConditions(rows: AttributeRow[]): AttributeCondition[] {
  return Object.entries(rowsToAttributes(rows)).map(([key, equals]) => ({ key, equals }))
}

/** `plan: pro · seats: 12`, for a table cell. */
export function describeAttributes(attributes: Attributes | undefined): string {
  return Object.entries(attributes ?? {})
    .map(([key, value]) => `${key}: ${String(value)}`)
    .join(' · ')
}

/** A segment in one comparable shape: times as milliseconds, empty parts dropped. */
export function canonicalSegment(segment: Segment | undefined) {
  const time = (at: string | null | undefined) => (at ? new Date(at).getTime() : null)
  return JSON.stringify({
    tags: segment?.tags ?? [],
    attributes: (segment?.attributes ?? []).map((c) => [c.key, c.equals]),
    joined_after: time(segment?.joined_after),
    joined_before: time(segment?.joined_before),
  })
}

export function isEveryone(segment: Segment | undefined): boolean {
  return canonicalSegment(segment) === canonicalSegment(undefined)
}

/** `Tagged “vip” · plan is pro · joined after Nov 1, 2026`, or `Every subscriber`. */
export function describeSegment(segment: Segment | undefined): string {
  if (!segment || isEveryone(segment)) return 'Every subscriber'
  const parts: string[] = []
  const tags = segment.tags ?? []
  if (tags.length) parts.push(`Tagged ${tags.map((t) => `“${t}”`).join(' and ')}`)
  for (const c of segment.attributes ?? []) parts.push(`${c.key} is ${String(c.equals)}`)
  if (segment.joined_after) parts.push(`joined on or after ${formatDate(segment.joined_after)}`)
  if (segment.joined_before) parts.push(`joined before ${formatDate(segment.joined_before)}`)
  return parts.join(' · ')
}

/** `YYYY-MM-DD` in local time, as `<input type="date">` wants it. */
export function dateInput(at: string | null | undefined): string {
  if (!at) return ''
  const date = new Date(at)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** Local midnight of a `YYYY-MM-DD` input, as RFC 3339, or null when empty. */
export function fromDateInput(value: string): string | null {
  return value === '' ? null : new Date(`${value}T00:00`).toISOString()
}
