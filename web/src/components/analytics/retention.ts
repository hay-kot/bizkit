/**
 * Background classes for a retention cell, stepped by share so lint-checked
 * Tailwind classes cover every value. Dark steps switch to the content
 * color for contrast.
 */
const SHADES = [
  'bg-primary/5',
  'bg-primary/10',
  'bg-primary/20',
  'bg-primary/30',
  'bg-primary/40',
  'bg-primary/60 text-primary-content',
  'bg-primary/80 text-primary-content',
  'bg-primary text-primary-content',
] as const

/** The shade for a share from 0 to 1; week 0 is usually 1 and gets the darkest. */
export function shadeClass(share: number): string {
  const clamped = Math.min(1, Math.max(0, share))
  const index = Math.min(SHADES.length - 1, Math.floor(clamped * SHADES.length))
  return SHADES[index] ?? SHADES[0]
}
