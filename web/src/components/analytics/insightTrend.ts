import { computed, type ComputedRef, type MaybeRefOrGetter, toValue } from 'vue'

import { type DayRange, previousPeriod } from '@/lib/range'
import { type InsightQuery, useInsightResult } from '@/queries/analytics'

import { insightView } from './insightView'

/**
 * The same query over the period just before `range`, for a number's
 * change. It runs only while `enabled`. `previous` stays undefined while
 * it loads, when it fails (the number shows without a change), and when
 * the earlier range starts before raw retention, since its number would
 * be short and every period would read as a rise.
 */
export function useInsightTrend(
  workspaceId: MaybeRefOrGetter<string>,
  projectId: MaybeRefOrGetter<string>,
  range: MaybeRefOrGetter<DayRange>,
  query: MaybeRefOrGetter<InsightQuery>,
  enabled: MaybeRefOrGetter<boolean>,
): { previous: ComputedRef<number | undefined> } {
  const result = useInsightResult(
    workspaceId,
    projectId,
    () => previousPeriod(toValue(range)),
    query,
    enabled,
  )
  const previous = computed(() => {
    const data = result.data.value
    if (!toValue(enabled) || !data || result.error.value) return undefined
    if (data.view === 'count' || data.raw_since) return undefined
    return insightView(data, Date.now()).trendValue
  })
  return { previous }
}
