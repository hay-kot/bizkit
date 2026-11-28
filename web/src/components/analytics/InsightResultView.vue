<script setup lang="ts">
import { useIntervalFn, useNow } from '@vueuse/core'
import { ArrowDown, ArrowUp, Minus } from 'lucide-vue-next'
import { computed } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import BarList from '@/components/ui/BarList.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import TimeSeriesChart from '@/components/ui/TimeSeriesChart.vue'
import { changeOf } from '@/lib/change'
import type { InsightDisplay, InsightResult } from '@/queries/analytics'

import { insightView, resolveDisplay } from './insightView'

/**
 * An insight's numbers, drawn by `display` (see `resolveDisplay`): a line
 * or bars over time under the headline, a ranked list, or the number on
 * its own with its change from `previous`, the same query's value over the
 * previous period. `compact` is for dashboard tiles: charts grow to fill
 * a tile that a taller neighbour stretches and drop the headline, which
 * the number display carries.
 */
const props = withDefaults(
  defineProps<{
    result: InsightResult | undefined
    loading: boolean
    error: unknown
    compact?: boolean
    display?: InsightDisplay
    previous?: number
  }>(),
  { compact: false, display: 'auto', previous: undefined },
)

// A minute is fine enough to move the open bucket on without a reload.
const now = useNow({ scheduler: (tick) => useIntervalFn(tick, 60_000) })
const view = computed(() =>
  props.result ? insightView(props.result, now.value.getTime()) : undefined,
)
const drawn = computed(() =>
  props.result ? resolveDisplay(props.display, props.result.view) : undefined,
)
const change = computed(() => {
  const value = view.value?.trendValue
  if (value === undefined || props.previous === undefined) return undefined
  return changeOf(value, props.previous, view.value?.trendMode)
})
const trendText = computed(() => {
  const c = change.value
  if (!c) return ''
  if (c.direction === 'new') return 'New this period'
  if (c.direction === 'flat') return 'No change from the previous period'
  return `${c.direction === 'up' ? 'Up' : 'Down'} ${c.text} from the previous period`
})
const tones = {
  up: 'text-success',
  new: 'text-success',
  down: 'text-error',
  flat: 'text-base-content/70',
}
const icons = { up: ArrowUp, new: ArrowUp, down: ArrowDown, flat: Minus }
</script>

<template>
  <ErrorAlert v-if="error" :error="error" />
  <SkeletonRows v-else-if="loading && !view" :rows="compact ? 3 : 5" />
  <div
    v-else-if="view && drawn === 'number'"
    class="flex flex-col items-center justify-center gap-2 py-6 text-center"
    :class="{ 'flex-1': compact }"
    :aria-busy="loading"
  >
    <p class="text-sm text-base-content/70">{{ view.headline.label }}</p>
    <p class="font-semibold tabular-nums" :class="compact ? 'text-4xl' : 'text-5xl'">
      {{ view.headline.value }}
    </p>
    <p v-if="view.caption" class="text-sm text-base-content/70">{{ view.caption }}</p>
    <p
      v-if="change"
      class="flex items-center gap-1.5 text-sm tabular-nums"
      :class="tones[change.direction]"
      data-trend
    >
      <component :is="icons[change.direction]" class="size-4" aria-hidden="true" />
      <span aria-hidden="true">{{ trendText }}</span>
      <span class="sr-only">{{ change.spoken }}</span>
    </p>
    <p v-if="view.missingBefore" class="text-sm text-warning">
      Raw events start on {{ view.missingBefore }}, so earlier days have no data.
    </p>
  </div>
  <div
    v-else-if="view"
    class="flex flex-col gap-4"
    :class="{ 'flex-1': compact }"
    :aria-busy="loading"
  >
    <div v-if="!compact || !view.chart" class="flex flex-col gap-1">
      <p class="text-sm text-base-content/70">{{ view.headline.label }}</p>
      <p class="font-semibold tabular-nums" :class="compact ? 'text-2xl' : 'text-3xl'">
        {{ view.headline.value }}
      </p>
      <p v-if="view.caption" class="text-sm text-base-content/70">{{ view.caption }}</p>
    </div>
    <p v-if="view.missingBefore" class="text-sm text-warning">
      Raw events start on {{ view.missingBefore }}, so earlier days have no data. Keep lifetime
      counts in person traits and ask a People insight.
    </p>
    <TimeSeriesChart
      v-if="view.chart"
      :points="view.chart.points"
      :series="view.chart.series"
      :kind="drawn === 'bar' ? 'bar' : 'line'"
      :partial-from="view.chart.partialFrom"
      :fill="compact"
    />
    <BarList
      v-if="view.bars"
      :items="view.bars.items"
      :value-label="view.bars.valueLabel"
      :limit="11"
    />
  </div>
</template>
