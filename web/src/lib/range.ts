/**
 * Report date ranges. Days are `YYYY-MM-DD` strings in UTC and both ends are
 * inclusive, matching the analytics API.
 */

export type RangePreset = 'today' | '7d' | '30d' | '90d' | 'month' | 'last-month' | 'custom'

export interface DayRange {
  from: string
  to: string
}

export interface DateRange extends DayRange {
  preset: RangePreset
}

export const RANGE_PRESETS: { value: RangePreset; label: string }[] = [
  { value: 'today', label: 'Today' },
  { value: '7d', label: 'Last 7 days' },
  { value: '30d', label: 'Last 30 days' },
  { value: '90d', label: 'Last 90 days' },
  { value: 'month', label: 'This month' },
  { value: 'last-month', label: 'Last month' },
  { value: 'custom', label: 'Custom' },
]

const DAY_MS = 86_400_000

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
    case '7d':
      return { from: addDays(today, -6), to: today }
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
