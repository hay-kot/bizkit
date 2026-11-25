/**
 * The field editor's geometry: placing, moving, resizing and nudging field
 * rects in page fractions (0..1 from the top-left corner of the displayed
 * page). Every result stays on the page and keeps each side at least
 * `MIN_SIDE`, the same bounds `signing.requests.fields.put` checks.
 */
import type { Rect } from '@/lib/pdf'
import type { SigningFieldKind } from '@/queries/signing'

/** The smallest side of a field, as the server checks it. */
export const MIN_SIDE = 0.01
/** What one arrow key press moves or resizes a field by. */
export const NUDGE = 0.005
/** The same with Shift held. */
export const NUDGE_FAST = 0.05

/**
 * Where a new field of each kind starts, sized for a letter or A4 page at
 * about 12 pt text. The sender resizes from there.
 */
export const FIELD_SIZES: Record<SigningFieldKind, { width: number; height: number }> = {
  signature: { width: 0.3, height: 0.06 },
  initials: { width: 0.1, height: 0.05 },
  date: { width: 0.18, height: 0.035 },
  text: { width: 0.3, height: 0.035 },
  checkbox: { width: 0.03, height: 0.025 },
}

/** Rects are kept to 4 decimals, counted here in whole steps. */
const STEPS = 10_000
const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value))
const finite = (value: number, fallback: number) => (Number.isFinite(value) ? value : fallback)

/**
 * One side's start and length, rounded to 4 decimals, so the stored rect
 * reads cleanly. The server refuses `start + length > 1`, and float sums
 * can land a hair past 1, so the start steps back until the sum fits.
 */
function side(start: number, length: number): [number, number] {
  const size = Math.round(clamp(finite(length, MIN_SIDE), MIN_SIDE, 1) * STEPS)
  let at = Math.round(clamp(finite(start, 0), 0, 1) * STEPS)
  at = Math.min(at, STEPS - size)
  while (at > 0 && at / STEPS + size / STEPS > 1) at--
  return [at / STEPS, size / STEPS]
}

/** `rect` moved and shrunk as little as needed to sit on the page. */
export function clampRect(rect: Rect): Rect {
  const [x, width] = side(rect.x, rect.width)
  const [y, height] = side(rect.y, rect.height)
  return { x, y, width, height }
}

/** `rect` moved by `dx`, `dy`, stopping at the page's edges. */
export function moveRect(rect: Rect, dx: number, dy: number): Rect {
  return clampRect({ ...rect, x: rect.x + dx, y: rect.y + dy })
}

/**
 * `rect` grown by `dw`, `dh` from its top-left corner, which stays put: at
 * least `MIN_SIDE` each way, and no further than the page's right and
 * bottom edges.
 */
export function resizeRect(rect: Rect, dw: number, dh: number): Rect {
  const width = clamp(rect.width + dw, MIN_SIDE, 1 - rect.x)
  const height = clamp(rect.height + dh, MIN_SIDE, 1 - rect.y)
  return clampRect({ ...rect, width, height })
}

/** A new field of `kind`, centred on the point (`cx`, `cy`), on the page. */
export function placeRect(kind: SigningFieldKind, cx: number, cy: number): Rect {
  const { width, height } = FIELD_SIZES[kind]
  return clampRect({ x: cx - width / 2, y: cy - height / 2, width, height })
}

const ARROWS: Record<string, [number, number]> = {
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
  ArrowUp: [0, -1],
  ArrowDown: [0, 1],
}

/**
 * The rect after an arrow key: a move, or with Alt a resize (Right and Down
 * grow, Left and Up shrink). Shift makes the step `NUDGE_FAST`. Gives
 * `undefined` for any other key.
 */
export function nudgeRect(
  rect: Rect,
  key: string,
  modifiers: { shiftKey?: boolean; altKey?: boolean } = {},
): Rect | undefined {
  const arrow = ARROWS[key]
  if (!arrow) return undefined
  const step = modifiers.shiftKey ? NUDGE_FAST : NUDGE
  const [dx, dy] = [arrow[0] * step, arrow[1] * step]
  return modifiers.altKey ? resizeRect(rect, dx, dy) : moveRect(rect, dx, dy)
}

/** A pointer drag in pixels on a page of `size` pixels, as page fractions. */
export function dragDelta(
  from: { x: number; y: number },
  to: { x: number; y: number },
  size: { width: number; height: number },
): [number, number] {
  if (size.width <= 0 || size.height <= 0) return [0, 0]
  return [(to.x - from.x) / size.width, (to.y - from.y) / size.height]
}
