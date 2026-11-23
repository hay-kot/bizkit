import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import {
  type DateRange,
  type RangePreset,
  rangeFromQuery,
  rangeToQuery,
  utcToday,
} from '@/lib/range'

/**
 * A report page's date range and comparison flag, kept in the URL query so
 * a link shares the exact view. Writes replace the history entry, so Back
 * leaves the page instead of stepping through ranges.
 */
export function useReportRange(fallback: RangePreset = '30d') {
  const route = useRoute()
  const router = useRouter()
  const today = utcToday()

  const range = computed<DateRange>({
    get: () => rangeFromQuery(route.query, today, fallback),
    set: (next) => void router.replace({ query: { ...route.query, ...rangeToQuery(next) } }),
  })

  const compare = computed<boolean>({
    get: () => route.query.compare === '1',
    set: (on) => void router.replace({ query: { ...route.query, compare: on ? '1' : undefined } }),
  })

  return { today, range, compare }
}
