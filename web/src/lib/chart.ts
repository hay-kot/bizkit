/**
 * Round axis ticks from 0 up to at least `max`: steps of 1, 2 or 5 times a
 * power of ten, about `count` of them. A max of 0 gives `[0, 1]` so the
 * axis still has a scale.
 */
export function niceTicks(max: number, count = 4): number[] {
  if (!(max > 0)) return [0, 1]
  const rough = max / (count - 1)
  const magnitude = 10 ** Math.floor(Math.log10(rough))
  const normal = rough / magnitude
  const step = (normal <= 1 ? 1 : normal <= 2 ? 2 : normal <= 5 ? 5 : 10) * magnitude
  const top = Math.ceil(max / step) * step
  const ticks: number[] = []
  for (let tick = 0; tick <= top + step / 2; tick += step) ticks.push(tick)
  return ticks
}

/** Up to `count` indexes spread evenly over `length` items, first and last included. */
export function spreadIndexes(length: number, count = 5): number[] {
  if (length <= count) return Array.from({ length }, (_, i) => i)
  const last = length - 1
  return Array.from({ length: count }, (_, i) => Math.round((i * last) / (count - 1)))
}

/** An SVG point: x, then y. */
export type Coord = readonly [number, number]

// Two decimals keep paths short without a visible change.
function n(value: number): number {
  return Math.round(value * 100) / 100
}

function sign(value: number): number {
  return value < 0 ? -1 : value > 0 ? 1 : 0
}

/** The tangent at a middle point (Steffen, as in d3's monotoneX). */
function innerTangent(a: Coord, b: Coord, c: Coord): number {
  const h0 = b[0] - a[0]
  const h1 = c[0] - b[0]
  const s0 = h0 ? (b[1] - a[1]) / h0 : 0
  const s1 = h1 ? (c[1] - b[1]) / h1 : 0
  const p = h0 + h1 ? (s0 * h1 + s1 * h0) / (h0 + h1) : 0
  return (sign(s0) + sign(s1)) * Math.min(Math.abs(s0), Math.abs(s1), 0.5 * Math.abs(p)) || 0
}

/** The tangent at an end, from its neighbour's tangent. */
function endTangent(a: Coord, b: Coord, neighbour: number): number {
  const h = b[0] - a[0]
  return h ? (3 * (b[1] - a[1])) / h / 2 - neighbour / 2 : neighbour
}

/**
 * A monotone cubic path through `coords` (x increasing), one `C` command
 * per segment: segment `i` runs from point `i` to point `i + 1`. Fewer
 * than two coords give none. Monotone, so the curve never passes above a
 * peak or below a trough of the data: a count never seems to dip under 0.
 */
export function monotoneSegments(coords: readonly Coord[]): string[] {
  const count = coords.length
  if (count < 2) return []
  const tangents: number[] = new Array<number>(count).fill(0)
  if (count === 2) {
    const [a, b] = coords as [Coord, Coord]
    const slope = b[0] - a[0] ? (b[1] - a[1]) / (b[0] - a[0]) : 0
    tangents.fill(slope)
  } else {
    for (let i = 1; i < count - 1; i++) {
      tangents[i] = innerTangent(coords[i - 1]!, coords[i]!, coords[i + 1]!)
    }
    tangents[0] = endTangent(coords[0]!, coords[1]!, tangents[1]!)
    tangents[count - 1] = endTangent(coords[count - 2]!, coords[count - 1]!, tangents[count - 2]!)
  }
  const segments: string[] = []
  for (let i = 0; i < count - 1; i++) {
    const [x0, y0] = coords[i]!
    const [x1, y1] = coords[i + 1]!
    const dx = (x1 - x0) / 3
    segments.push(
      `C${n(x0 + dx)},${n(y0 + dx * tangents[i]!)} ${n(x1 - dx)},${n(y1 - dx * tangents[i + 1]!)} ${n(x1)},${n(y1)}`,
    )
  }
  return segments
}

/**
 * A path from point `from` to point `to` through `segments` (from
 * `monotoneSegments(coords)`). Empty when the range holds no point.
 */
export function pathBetween(
  coords: readonly Coord[],
  segments: readonly string[],
  from: number,
  to: number,
): string {
  const start = coords[from]
  if (!start || to < from) return ''
  return [`M${n(start[0])},${n(start[1])}`, ...segments.slice(from, to)].join(' ')
}

/**
 * A bar from its top-left corner, with only the top corners rounded. Empty
 * for a bar with no height.
 */
export function barPath(
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
): string {
  if (!(height > 0) || !(width > 0)) return ''
  const r = Math.min(radius, width / 2, height)
  const bottom = y + height
  return [
    `M${n(x)},${n(bottom)}`,
    `V${n(y + r)}`,
    `Q${n(x)},${n(y)} ${n(x + r)},${n(y)}`,
    `H${n(x + width - r)}`,
    `Q${n(x + width)},${n(y)} ${n(x + width)},${n(y + r)}`,
    `V${n(bottom)}`,
    'Z',
  ].join(' ')
}

/**
 * The index of the first bucket that hasn't ended at `now`: its start plus
 * `intervalMs` is after `now`. Undefined when every bucket has ended.
 * `starts` are ISO times in order; dropping buckets that haven't begun is
 * the caller's job.
 */
export function openFrom(
  starts: readonly string[],
  intervalMs: number,
  now: number,
): number | undefined {
  const index = starts.findIndex((start) => Date.parse(start) + intervalMs > now)
  return index === -1 ? undefined : index
}
