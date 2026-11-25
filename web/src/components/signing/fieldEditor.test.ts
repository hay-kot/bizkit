import { describe, expect, it } from 'vitest'

import {
  clampRect,
  dragDelta,
  FIELD_SIZES,
  MIN_SIDE,
  moveRect,
  NUDGE,
  NUDGE_FAST,
  nudgeRect,
  placeRect,
  resizeRect,
} from './fieldEditor'

/** The server's checks in `fields::check_rect`. */
function onPage(rect: { x: number; y: number; width: number; height: number }) {
  return (
    rect.x >= 0 &&
    rect.x < 1 &&
    rect.y >= 0 &&
    rect.y < 1 &&
    rect.width >= MIN_SIDE &&
    rect.height >= MIN_SIDE &&
    rect.x + rect.width <= 1 &&
    rect.y + rect.height <= 1
  )
}

const box = { x: 0.4, y: 0.4, width: 0.2, height: 0.1 }

describe('moving a field', () => {
  it('moves by the delta inside the page', () => {
    expect(moveRect(box, 0.1, -0.2)).toEqual({ x: 0.5, y: 0.2, width: 0.2, height: 0.1 })
  })

  it('stops at every edge and keeps its size', () => {
    expect(moveRect(box, 5, 5)).toEqual({ x: 0.8, y: 0.9, width: 0.2, height: 0.1 })
    expect(moveRect(box, -5, -5)).toEqual({ x: 0, y: 0, width: 0.2, height: 0.1 })
  })

  it('never lands past the right edge after rounding', () => {
    for (let width = 0.01; width < 1; width += 0.0137) {
      const moved = moveRect({ x: 0, y: 0, width, height: 0.1 }, 2, 2)
      expect(onPage(moved), JSON.stringify(moved)).toBe(true)
    }
  })
})

describe('resizing a field', () => {
  it('grows from the top-left corner', () => {
    expect(resizeRect(box, 0.1, 0.05)).toEqual({ x: 0.4, y: 0.4, width: 0.3, height: 0.15 })
  })

  it('stops at the right and bottom edges without moving', () => {
    expect(resizeRect(box, 2, 2)).toEqual({ x: 0.4, y: 0.4, width: 0.6, height: 0.6 })
  })

  it('keeps at least 0.01 each way', () => {
    expect(resizeRect(box, -1, -1)).toEqual({ x: 0.4, y: 0.4, width: MIN_SIDE, height: MIN_SIDE })
  })
})

describe('placing a field', () => {
  it('centres the default size on the point', () => {
    const { width, height } = FIELD_SIZES.signature
    const placed = placeRect('signature', 0.5, 0.5)
    expect(placed.width).toBe(width)
    expect(placed.height).toBe(height)
    expect(placed.x).toBeCloseTo(0.5 - width / 2)
    expect(placed.y).toBeCloseTo(0.5 - height / 2)
  })

  it('pulls a field dropped at a corner onto the page', () => {
    for (const kind of Object.keys(FIELD_SIZES) as (keyof typeof FIELD_SIZES)[]) {
      for (const [cx, cy] of [
        [0, 0],
        [1, 1],
        [1, 0],
        [0, 1],
      ] as const) {
        expect(onPage(placeRect(kind, cx, cy)), `${kind} at ${cx},${cy}`).toBe(true)
      }
    }
  })
})

describe('clamping', () => {
  it('shrinks a field larger than the page', () => {
    expect(clampRect({ x: -1, y: 0.5, width: 3, height: 0.2 })).toEqual({
      x: 0,
      y: 0.5,
      width: 1,
      height: 0.2,
    })
  })

  it('turns bad numbers into a small field at the origin', () => {
    expect(clampRect({ x: Number.NaN, y: 0, width: Number.NaN, height: 0.1 })).toEqual({
      x: 0,
      y: 0,
      width: MIN_SIDE,
      height: 0.1,
    })
  })
})

describe('keyboard nudges', () => {
  it('moves by a small step, or a large one with Shift', () => {
    expect(nudgeRect(box, 'ArrowRight')?.x).toBeCloseTo(0.4 + NUDGE)
    expect(nudgeRect(box, 'ArrowUp')?.y).toBeCloseTo(0.4 - NUDGE)
    expect(nudgeRect(box, 'ArrowLeft', { shiftKey: true })?.x).toBeCloseTo(0.4 - NUDGE_FAST)
    expect(nudgeRect(box, 'ArrowDown', { shiftKey: true })?.y).toBeCloseTo(0.4 + NUDGE_FAST)
  })

  it('resizes with Alt', () => {
    const wider = nudgeRect(box, 'ArrowRight', { altKey: true })
    expect(wider?.x).toBe(0.4)
    expect(wider?.width).toBeCloseTo(0.2 + NUDGE)
    const shorter = nudgeRect(box, 'ArrowUp', { altKey: true, shiftKey: true })
    expect(shorter?.height).toBeCloseTo(0.1 - NUDGE_FAST)
  })

  it('stops at the edges', () => {
    const corner = { x: 0.99 - 0.2, y: 0, width: 0.2, height: 0.1 }
    let moved = corner
    for (let i = 0; i < 10; i++) moved = nudgeRect(moved, 'ArrowRight', { shiftKey: true })!
    expect(moved.x + moved.width).toBeLessThanOrEqual(1)
    expect(nudgeRect({ ...corner, x: 0.8 }, 'ArrowUp')?.y).toBe(0)
    let small = box
    for (let i = 0; i < 10; i++)
      small = nudgeRect(small, 'ArrowLeft', { altKey: true, shiftKey: true })!
    expect(small.width).toBe(MIN_SIDE)
  })

  it('ignores other keys', () => {
    expect(nudgeRect(box, 'Enter')).toBeUndefined()
  })
})

describe('pointer drags', () => {
  it('turns pixels into page fractions', () => {
    expect(dragDelta({ x: 10, y: 10 }, { x: 70, y: -20 }, { width: 600, height: 300 })).toEqual([
      0.1, -0.1,
    ])
  })

  it('gives nothing on a page with no size yet', () => {
    expect(dragDelta({ x: 0, y: 0 }, { x: 5, y: 5 }, { width: 0, height: 100 })).toEqual([0, 0])
  })
})
