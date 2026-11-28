import { formatNumber, formatPercent } from './format'

/** A value against the previous period's. */
export interface Change {
  direction: 'up' | 'down' | 'flat' | 'new'
  /** `19.2%`, `5 pts`, or `new` for a rise from 0. */
  text: string
  /** `down 19.2% from the previous period`, for screen readers. */
  spoken: string
}

const NO_CHANGE = 'no change from the previous period'

/**
 * How `value` moved from `previous`. `relative` gives the change as a
 * share of `previous`; a rise from 0 has no finite share and reads `new`.
 * `points` reads both as shares (0 to 1) and gives the difference in
 * percentage points, since "up 25%" of a 20% share reads like 45%.
 */
export function changeOf(
  value: number,
  previous: number,
  mode: 'relative' | 'points' = 'relative',
): Change {
  if (mode === 'points') {
    const points = Math.round((value - previous) * 1000) / 10
    if (points === 0) return { direction: 'flat', text: '0 pts', spoken: NO_CHANGE }
    const direction = points > 0 ? 'up' : 'down'
    const amount = formatNumber(Math.abs(points))
    return {
      direction,
      text: `${amount} pts`,
      spoken: `${direction} ${amount} percentage points from the previous period`,
    }
  }
  if (previous === 0) {
    return value === 0
      ? { direction: 'flat', text: formatPercent(0), spoken: NO_CHANGE }
      : { direction: 'new', text: 'new', spoken: 'new since the previous period' }
  }
  const ratio = (value - previous) / Math.abs(previous)
  const text = formatPercent(Math.abs(ratio))
  if (ratio === 0) return { direction: 'flat', text, spoken: NO_CHANGE }
  const direction = ratio > 0 ? 'up' : 'down'
  return { direction, text, spoken: `${direction} ${text} from the previous period` }
}
