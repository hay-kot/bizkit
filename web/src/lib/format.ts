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
