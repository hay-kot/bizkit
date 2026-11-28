import type { StepResult } from '@/queries/analytics'

import { describeStep } from './funnelDraft'

/** One funnel step placed in a plot, coordinates from the plot's top-left corner. */
export interface FunnelBar {
  /** The step's number, from 1. */
  number: number
  label: string
  people: number
  /** Share of step 1's people, 0 to 1: the bar's height. */
  fromFirst: number
  /** Share of the previous step's people, 0 to 1. Not on step 1. */
  fromPrevious?: number
  /** People the previous step had and this one lost, at least 0. Not on step 1. */
  dropped?: number
  /** The band's left edge; the bar is centered in it. */
  bandX: number
  bandWidth: number
  x: number
  width: number
  top: number
  /** The previous step's top, so the lost share spans `lostTop` to `top`. Not on step 1. */
  lostTop?: number
}

/** A bar's width as a share of its band; the rest is the gap the chips sit in. */
const BAR_SHARE = 0.4

function clampShare(value: number): number {
  return Math.min(1, Math.max(0, value))
}

/**
 * Bars for a funnel's steps in a plot of `plotWidth` by `plotHeight`, each
 * as tall as its share of step 1. With nobody at step 1 every bar is 0 and
 * no step reports a drop-off.
 */
export function funnelBars(
  steps: readonly StepResult[],
  plotWidth: number,
  plotHeight: number,
): FunnelBar[] {
  const entered = steps[0]?.people ?? 0
  const bandWidth = plotWidth / Math.max(1, steps.length)
  const width = bandWidth * BAR_SHARE
  const tops: number[] = []
  return steps.map((result, i) => {
    const fromFirst = entered > 0 ? clampShare(result.from_first) : 0
    const top = plotHeight * (1 - fromFirst)
    tops.push(top)
    const previous = i > 0 ? steps[i - 1] : undefined
    const bandX = bandWidth * i
    const bar: FunnelBar = {
      number: i + 1,
      label: describeStep(result.step),
      people: result.people,
      fromFirst,
      bandX,
      bandWidth,
      x: bandX + (bandWidth - width) / 2,
      width,
      top,
    }
    if (previous && entered > 0) {
      bar.fromPrevious = clampShare(result.from_previous)
      bar.dropped = Math.max(0, previous.people - result.people)
      bar.lostTop = tops[i - 1]
    }
    return bar
  })
}

/**
 * Narrower bands than this leave no room between bars for a chip that
 * reads "1,234 dropped off" on one line.
 */
export const CHIP_MIN_BAND = 160
