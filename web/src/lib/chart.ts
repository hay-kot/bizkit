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
