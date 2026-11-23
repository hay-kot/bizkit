/** The one place dates and times turn into text, so the app stays consistent. */

const dateFormat = new Intl.DateTimeFormat(undefined, {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
})
const shortFormat = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
const dateTimeFormat = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'medium',
})
const relativeFormat = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['second', 60],
  ['minute', 60],
  ['hour', 24],
  ['day', 7],
]

/** `Nov 22, 2026`, or `fallback` when there is no value. */
export function formatDate(value: string | null | undefined, fallback = '—'): string {
  return value ? dateFormat.format(new Date(value)) : fallback
}

/** `Nov 22`, for dates close to now. */
export function formatShortDate(value: string | null | undefined, fallback = '—'): string {
  return value ? shortFormat.format(new Date(value)) : fallback
}

const utcDayFormat = new Intl.DateTimeFormat(undefined, {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
  timeZone: 'UTC',
})
const utcShortDayFormat = new Intl.DateTimeFormat(undefined, {
  month: 'short',
  day: 'numeric',
  timeZone: 'UTC',
})

/**
 * A UTC calendar day (`2026-11-22`, or a timestamp read in UTC) as
 * `Nov 22, 2026`. Analytics counts by UTC day, so its labels must not
 * shift into the viewer's time zone.
 */
export function formatUtcDay(value: string): string {
  return utcDayFormat.format(new Date(`${value.slice(0, 10)}T00:00:00Z`))
}

/** A UTC calendar day as `Nov 22`. */
export function formatShortUtcDay(value: string): string {
  return utcShortDayFormat.format(new Date(`${value.slice(0, 10)}T00:00:00Z`))
}

/** True when `value` is before now. */
export function isPast(value: string): boolean {
  return new Date(value).getTime() < Date.now()
}

/** `Nov 22, 2026, 8:04:12 PM`, for a hover title next to a relative time. */
export function formatDateTime(value: string | null | undefined, fallback = '—'): string {
  return value ? dateTimeFormat.format(new Date(value)) : fallback
}

/**
 * `2 minutes ago`, `yesterday`, up to a week back; older times fall back
 * to `formatDate`. `now` is for tests.
 */
export function formatRelative(
  value: string | null | undefined,
  fallback = '—',
  now = Date.now(),
): string {
  if (!value) return fallback
  let amount = (new Date(value).getTime() - now) / 1000
  for (const [unit, size] of UNITS) {
    if (Math.abs(amount) < size) return relativeFormat.format(Math.trunc(amount), unit)
    amount /= size
  }
  return formatDate(value)
}

const numberFormat = new Intl.NumberFormat(undefined)
const compactFormat = new Intl.NumberFormat(undefined, {
  notation: 'compact',
  maximumFractionDigits: 1,
})
const percentFormat = new Intl.NumberFormat(undefined, {
  style: 'percent',
  maximumFractionDigits: 1,
})

/** `1,284`. */
export function formatNumber(value: number): string {
  return numberFormat.format(value)
}

/** `1.3K`, for chart axes where space is short. */
export function formatCompactNumber(value: number): string {
  return compactFormat.format(value)
}

/** A ratio as a percent: `0.037` gives `3.7%`. */
export function formatPercent(ratio: number): string {
  return percentFormat.format(ratio)
}
