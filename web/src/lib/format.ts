/** The one place dates and times turn into text, so the app stays consistent. */

const dateFormat = new Intl.DateTimeFormat(undefined, {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
})
const shortFormat = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })

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
