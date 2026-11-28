import { addDays } from '@/lib/range'
import type { Cohort } from '@/queries/analytics'

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

/** The last day of week `i` after the cohort week starting `week`. */
function weekEnd(week: string, i: number): string {
  return addDays(week, 7 * i + 6)
}

/** Week `i` of the cohort starting `week` has begun but not ended on `today` (UTC days). */
export function inProgress(week: string, i: number, today: string): boolean {
  return addDays(week, 7 * i) <= today && today <= weekEnd(week, i)
}

/**
 * The share per week over the cohorts where that week has ended, weighted
 * by cohort people: returns are `round(share * people)`. A week no cohort
 * has finished gives null, so a week still filling up doesn't pull it down.
 */
export function meanRow(
  cohorts: readonly Cohort[],
  today: string,
  weeks: number,
): (number | null)[] {
  return Array.from({ length: weeks }, (_, i) => {
    let people = 0
    let returned = 0
    for (const cohort of cohorts) {
      const share = cohort.weeks[i]
      if (share === null || share === undefined || weekEnd(cohort.week, i) >= today) continue
      people += cohort.people
      returned += Math.round(share * cohort.people)
    }
    return people > 0 ? returned / people : null
  })
}
