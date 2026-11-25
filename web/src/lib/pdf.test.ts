import { describe, expect, it, vi } from 'vitest'

vi.mock('pdfjs-dist/build/pdf.worker.min.mjs?url', () => ({ default: '' }))

import {
  fractionToPdfPoint,
  pdfPointToFraction,
  toFractions,
  toPixels,
  type ViewportLike,
} from './pdf'

/**
 * A viewport built the way pdf.js's `PageViewport` builds one, so the
 * helpers run against real rotation math without loading pdf.js.
 */
function viewport(viewBox: [number, number, number, number], rotation: number, scale = 1) {
  const [x0, y0, x1, y1] = viewBox
  const cx = (x0 + x1) / 2
  const cy = (y0 + y1) / 2
  const r = ((rotation % 360) + 360) % 360
  const turns: Record<number, [number, number, number, number]> = {
    0: [1, 0, 0, -1],
    90: [0, 1, 1, 0],
    180: [-1, 0, 0, 1],
    270: [0, -1, -1, 0],
  }
  const [a, b, c, d] = turns[r]!
  const swapped = a === 0
  const width = (swapped ? y1 - y0 : x1 - x0) * scale
  const height = (swapped ? x1 - x0 : y1 - y0) * scale
  const ox = (swapped ? Math.abs(cy - y0) : Math.abs(cx - x0)) * scale
  const oy = (swapped ? Math.abs(cx - x0) : Math.abs(cy - y0)) * scale
  const t = [
    a * scale,
    b * scale,
    c * scale,
    d * scale,
    ox - a * scale * cx - c * scale * cy,
    oy - b * scale * cx - d * scale * cy,
  ] as const
  const det = t[0] * t[3] - t[1] * t[2]
  return {
    width,
    height,
    convertToViewportPoint: (x: number, y: number) => [
      t[0] * x + t[2] * y + t[4],
      t[1] * x + t[3] * y + t[5],
    ],
    convertToPdfPoint: (x: number, y: number) => [
      (t[3] * (x - t[4]) - t[2] * (y - t[5])) / det,
      (-t[1] * (x - t[4]) + t[0] * (y - t[5])) / det,
    ],
  } satisfies ViewportLike
}

const letter: [number, number, number, number] = [0, 0, 612, 792]
const round = (point: number[]) => point.map((v) => Math.round(v * 1e6) / 1e6 + 0)
const roundRect = (rect: { x: number; y: number; width: number; height: number }) => {
  const [x, y, width, height] = round([rect.x, rect.y, rect.width, rect.height])
  return { x, y, width, height }
}

describe('toFractions and toPixels', () => {
  const size = { width: 800, height: 1000 }

  it('turn viewport pixels into page fractions and back', () => {
    const rect = { x: 80, y: 250, width: 200, height: 50 }
    const fractions = toFractions(rect, size)
    expect(roundRect(fractions)).toEqual({ x: 0.1, y: 0.25, width: 0.25, height: 0.05 })
    expect(roundRect(toPixels(fractions, size))).toEqual(rect)
  })

  it('cut a rect that reaches past the page at its edges', () => {
    expect(roundRect(toFractions({ x: -40, y: 900, width: 200, height: 300 }, size))).toEqual({
      x: 0,
      y: 0.9,
      width: 0.2,
      height: 0.1,
    })
  })

  it('give the same fractions at any zoom', () => {
    const rect = { x: 0.2, y: 0.3, width: 0.4, height: 0.1 }
    for (const scale of [0.5, 1, 2.5]) {
      const zoomed = { width: 612 * scale, height: 792 * scale }
      expect(roundRect(toFractions(toPixels(rect, zoomed), zoomed))).toEqual(rect)
    }
  })
})

describe('pdf points and page fractions', () => {
  it('put the top-left of an unrotated page at its top-left corner', () => {
    const view = viewport(letter, 0)
    expect(round(pdfPointToFraction(view, 0, 792))).toEqual([0, 0])
    expect(round(pdfPointToFraction(view, 612, 0))).toEqual([1, 1])
    expect(round(pdfPointToFraction(view, 306, 594))).toEqual([0.5, 0.25])
  })

  it.each([
    // Rotated a quarter turn clockwise, the page's bottom-left corner shows top left.
    [90, [0, 0], [0, 0]],
    [90, [612, 792], [1, 1]],
    [90, [0, 792], [1, 0]],
    [180, [612, 0], [0, 0]],
    [180, [0, 792], [1, 1]],
    [270, [612, 792], [0, 0]],
    [270, [0, 0], [1, 1]],
    [-90, [612, 792], [0, 0]],
    [450, [0, 0], [0, 0]],
  ])('map a page rotated %d degrees with a top-left origin', (rotation, [x, y], fraction) => {
    expect(round(pdfPointToFraction(viewport(letter, rotation), x!, y!))).toEqual(fraction)
  })

  it('swap the displayed width and height on quarter turns', () => {
    expect([viewport(letter, 90).width, viewport(letter, 90).height]).toEqual([792, 612])
    expect([viewport(letter, 180).width, viewport(letter, 180).height]).toEqual([612, 792])
  })

  it('measure from an offset crop box, not the media box origin', () => {
    const view = viewport([36, 36, 576, 756], 0, 2)
    expect(round(pdfPointToFraction(view, 36, 756))).toEqual([0, 0])
    expect(round(pdfPointToFraction(view, 306, 396))).toEqual([0.5, 0.5])
  })

  it.each([0, 90, 180, 270])('round-trip a point on a page rotated %d degrees', (rotation) => {
    const view = viewport(letter, rotation, 1.5)
    const [x, y] = fractionToPdfPoint(view, 0.2, 0.7)
    expect(round(pdfPointToFraction(view, x, y))).toEqual([0.2, 0.7])
  })
})
