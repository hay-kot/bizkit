import { useEventListener } from '@vueuse/core'
import { computed, onScopeDispose, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import {
  type DateRange,
  msToNextUtcDay,
  type RangePreset,
  rangeFromQuery,
  rangeToQuery,
  utcToday,
} from '@/lib/range'

/**
 * A report page's date range and comparison flag, kept in the URL query so
 * a link shares the exact view. Writes replace the history entry, so Back
 * leaves the page instead of stepping through ranges.
 *
 * `today` (UTC) moves at UTC midnight, so a page left open overnight rolls
 * its presets forward. Timers stall in background tabs, so it also checks
 * again when the tab becomes visible.
 */
export function useReportRange(fallback: RangePreset = '30d') {
  const route = useRoute()
  const router = useRouter()
  const current = ref(utcToday())
  const today = computed(() => current.value)

  let timer: ReturnType<typeof setTimeout> | undefined
  function refresh() {
    current.value = utcToday()
    clearTimeout(timer)
    timer = setTimeout(refresh, msToNextUtcDay())
  }
  refresh()
  useEventListener(document, 'visibilitychange', () => {
    if (document.visibilityState === 'visible') refresh()
  })
  onScopeDispose(() => clearTimeout(timer))

  const range = computed<DateRange>({
    get: () => rangeFromQuery(route.query, today.value, fallback),
    set: (next) => void router.replace({ query: { ...route.query, ...rangeToQuery(next) } }),
  })

  const compare = computed<boolean>({
    get: () => route.query.compare === '1',
    set: (on) => void router.replace({ query: { ...route.query, compare: on ? '1' : undefined } }),
  })

  return { today, range, compare }
}
