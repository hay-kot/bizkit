import { describe, expect, it } from 'vitest'

import type { StepResult } from '@/queries/analytics'

import { funnelBars } from './funnel'

function step(match: string, people: number, fromPrevious: number, fromFirst: number): StepResult {
  return {
    step: { kind: 'event', match },
    people,
    from_previous: fromPrevious,
    from_first: fromFirst,
  }
}

const steps = [step('visit', 100, 1, 1), step('signup', 50, 0.5, 0.5), step('pay', 10, 0.2, 0.1)]

describe('funnelBars', () => {
  it('scales bars to step 1 and centers them in their bands', () => {
    const bars = funnelBars(steps, 300, 200)
    expect(bars.map((bar) => bar.top)).toEqual([0, 100, 180])
    expect(bars.map((bar) => bar.bandX)).toEqual([0, 100, 200])
    expect(bars[0]!.bandWidth).toBe(100)
    expect(bars[1]!.width).toBe(40)
    expect(bars[1]!.x).toBe(130)
  })

  it('spans the lost share from the previous top and counts the drop-off', () => {
    const [first, second, third] = funnelBars(steps, 300, 200)
    expect(first!.lostTop).toBeUndefined()
    expect(first!.dropped).toBeUndefined()
    expect(second!.lostTop).toBe(0)
    expect(second!.dropped).toBe(50)
    expect(second!.fromPrevious).toBe(0.5)
    expect(third!.lostTop).toBe(100)
    expect(third!.dropped).toBe(40)
    expect(third!.label).toBe('pay')
  })

  it('clamps a drop-off that would go negative', () => {
    const bars = funnelBars([step('a', 5, 1, 1), step('b', 7, 1.4, 1.4)], 200, 100)
    expect(bars[1]!.dropped).toBe(0)
    expect(bars[1]!.top).toBe(0)
  })

  it('draws every bar at 0 when nobody entered', () => {
    const bars = funnelBars([step('a', 0, 1, 0), step('b', 0, 0, 0)], 200, 100)
    expect(bars.map((bar) => bar.top)).toEqual([100, 100])
    expect(bars[1]!.dropped).toBeUndefined()
    expect(bars[1]!.lostTop).toBeUndefined()
  })

  it('gives nothing for no steps', () => {
    expect(funnelBars([], 200, 100)).toEqual([])
  })
})
