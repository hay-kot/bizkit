/**
 * The color a signer's fields carry in the editor and the read-only view,
 * by the signer's place in the request's list. A request has at most 10
 * signers and there are 8 colors, so the 9th and 10th repeat the first
 * two; fields also name their signer, so color is never the only cue.
 * Full class names, so Tailwind finds them.
 */
const COLORS = [
  { box: 'border-chart-1 bg-chart-1/15', swatch: 'bg-chart-1' },
  { box: 'border-chart-2 bg-chart-2/15', swatch: 'bg-chart-2' },
  { box: 'border-chart-3 bg-chart-3/15', swatch: 'bg-chart-3' },
  { box: 'border-chart-4 bg-chart-4/15', swatch: 'bg-chart-4' },
  { box: 'border-warning bg-warning/20', swatch: 'bg-warning' },
  { box: 'border-error bg-error/15', swatch: 'bg-error' },
  { box: 'border-info bg-info/15', swatch: 'bg-info' },
  { box: 'border-neutral bg-neutral/15', swatch: 'bg-neutral' },
] as const

export type SignerColor = (typeof COLORS)[number]

export function signerColor(index: number): SignerColor {
  return COLORS[((index % COLORS.length) + COLORS.length) % COLORS.length]!
}
