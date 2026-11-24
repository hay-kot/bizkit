/**
 * Report date ranges. Days are `YYYY-MM-DD` strings in UTC and both ends are
 * inclusive, matching the analytics API.
 */

export type RangePreset =
  'today' | 'yesterday' | '7d' | '14d' | '30d' | '90d' | 'month' | 'last-month' | 'custom'

/**
 * The presets a saved insight can keep. `yesterday` and `14d` live only in
 * the URL, because the insights API stores the older set.
 */
export type SavedPreset = Exclude<RangePreset, 'yesterday' | '14d' | 'custom'>

export interface DayRange {
  from: string
  to: string
}

export interface DateRange extends DayRange {
  preset: RangePreset
}

export interface PresetOption {
  value: Exclude<RangePreset, 'custom'>
  label: string
}

/** The quick ranges, in the groups the picker shows them. */
export const PRESET_GROUPS: PresetOption[][] = [
  [
    { value: 'today', label: 'Today' },
    { value: 'yesterday', label: 'Yesterday' },
  ],
  [
    { value: '7d', label: 'Last 7 days' },
    { value: '14d', label: 'Last 14 days' },
    { value: '30d', label: 'Last 30 days' },
    { value: '90d', label: 'Last 90 days' },
  ],
  [
    { value: 'month', label: 'This month' },
    { value: 'last-month', label: 'Last month' },
  ],
]

export const RANGE_PRESETS: { value: RangePreset; label: string }[] = [
  ...PRESET_GROUPS.flat(),
  { value: 'custom', label: 'Custom' },
]

const SAVED_PRESETS: readonly RangePreset[] = ['today', '7d', '30d', '90d', 'month', 'last-month']

/** `preset` when a saved insight can keep it, else undefined. */
export function savedPreset(preset: RangePreset): SavedPreset | undefined {
  return SAVED_PRESETS.includes(preset) ? (preset as SavedPreset) : undefined
}

const DAY_MS = 86_400_000

/**
 * The most days a range may cover, counting both ends. The analytics API
 * refuses longer ranges (`MAX_RANGE_DAYS` in `crates/bizkit-analytics/src/stats.rs`
 * rejects `to` 400 or more days after `from`); keep the two in step.
 */
export const MAX_RANGE_DAYS = 400

function toTime(day: string): number {
  return Date.parse(`${day}T00:00:00Z`)
}

function toDay(time: number): string {
  return new Date(time).toISOString().slice(0, 10)
}

/** `day` moved by `days`, which may be negative. */
export function addDays(day: string, days: number): string {
  return toDay(toTime(day) + days * DAY_MS)
}

/** The number of days in the range, counting both ends. */
export function rangeLength({ from, to }: DayRange): number {
  return Math.round((toTime(to) - toTime(from)) / DAY_MS) + 1
}

/**
 * The days a preset covers, ending today. `custom` has no days of its own,
 * so it gives today alone; callers keep the user's dates instead.
 */
export function presetRange(preset: RangePreset, today: string): DayRange {
  switch (preset) {
    case 'yesterday': {
      const day = addDays(today, -1)
      return { from: day, to: day }
    }
    case '7d':
      return { from: addDays(today, -6), to: today }
    case '14d':
      return { from: addDays(today, -13), to: today }
    case '30d':
      return { from: addDays(today, -29), to: today }
    case '90d':
      return { from: addDays(today, -89), to: today }
    case 'month':
      return { from: `${today.slice(0, 8)}01`, to: today }
    case 'last-month': {
      const to = addDays(`${today.slice(0, 8)}01`, -1)
      return { from: `${to.slice(0, 8)}01`, to }
    }
    case 'today':
    case 'custom':
      return { from: today, to: today }
  }
}

/** The same number of days straight before `range`, for comparison. */
export function previousPeriod(range: DayRange): DayRange {
  return {
    from: addDays(range.from, -rangeLength(range)),
    to: addDays(range.from, -1),
  }
}

/**
 * `range` moved one whole length back (`-1`) or forward (`1`), so the days
 * just before or after it. A move never passes `today`: it stops there and
 * keeps the length. The result is always `custom`, since its dates no longer
 * follow today.
 */
export function shiftRange(range: DayRange, direction: -1 | 1, today: string): DateRange {
  const length = rangeLength(range)
  const to = minDay(addDays(range.to, direction * length), today)
  return { preset: 'custom', from: addDays(to, 1 - length), to }
}

/**
 * Twice the days of `range`, centered on it, like Grafana's zoom out, but
 * never more than `MAX_RANGE_DAYS`. The extra odd day goes before it, so one
 * day ending today widens to two days ending today. When the end would pass
 * `today`, the range moves back to end today and keeps its new length.
 */
export function zoomOut(range: DayRange, today: string): DateRange {
  const length = rangeLength(range)
  const zoomed = Math.min(2 * length, MAX_RANGE_DAYS)
  const to = minDay(addDays(range.to, Math.floor((zoomed - length) / 2)), today)
  return { preset: 'custom', from: addDays(to, 1 - zoomed), to }
}

/** False once `range` covers `MAX_RANGE_DAYS`, so zooming out would add nothing. */
export function canZoomOut(range: DayRange): boolean {
  return rangeLength(range) < MAX_RANGE_DAYS
}

function minDay(a: string, b: string): string {
  return a < b ? a : b
}

/** Milliseconds from `now` to the next UTC midnight, when `utcToday` changes. */
export function msToNextUtcDay(now: Date = new Date()): number {
  return toTime(utcToday(now)) + DAY_MS - now.getTime()
}

/** Today in UTC, as `YYYY-MM-DD`. */
export function utcToday(now: Date = new Date()): string {
  return toDay(now.getTime())
}

const DAY_PATTERN = /^\d{4}-\d{2}-\d{2}$/

function isDay(value: unknown): value is string {
  return typeof value === 'string' && DAY_PATTERN.test(value) && !Number.isNaN(toTime(value))
}

/**
 * The range a report URL asks for: `?range=<preset>`, or `?range=custom`
 * with `from` and `to`. Anything else gives `fallback`.
 */
export function rangeFromQuery(
  query: Record<string, unknown>,
  today: string,
  fallback: RangePreset,
): DateRange {
  const preset = RANGE_PRESETS.find((p) => p.value === query.range)?.value
  if (preset === 'custom') {
    const { from, to } = query
    if (isDay(from) && isDay(to) && from <= to) return { preset, from, to }
  } else if (preset) {
    return { preset, ...presetRange(preset, today) }
  }
  return { preset: fallback, ...presetRange(fallback, today) }
}

/** The query that `rangeFromQuery` reads back as `range`. */
export function rangeToQuery(range: DateRange): Record<string, string | undefined> {
  return range.preset === 'custom'
    ? { range: 'custom', from: range.from, to: range.to }
    : { range: range.preset, from: undefined, to: undefined }
}
