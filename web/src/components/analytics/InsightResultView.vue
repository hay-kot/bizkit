<script setup lang="ts">
import { computed } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import BarList from '@/components/ui/BarList.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import TimeSeriesChart from '@/components/ui/TimeSeriesChart.vue'
import type { InsightResult } from '@/queries/analytics'

import { insightView } from './insightView'

/**
 * An insight's numbers: the headline, then a chart over time or a ranked
 * list, by the result's `view`. `compact` is for dashboard tiles: the
 * chart grows to fill a tile that a taller neighbour stretches.
 */
const props = withDefaults(
  defineProps<{
    result: InsightResult | undefined
    loading: boolean
    error: unknown
    compact?: boolean
  }>(),
  { compact: false },
)

const view = computed(() => (props.result ? insightView(props.result) : undefined))
</script>

<template>
  <ErrorAlert v-if="error" :error="error" />
  <SkeletonRows v-else-if="loading && !view" :rows="compact ? 3 : 5" />
  <div
    v-else-if="view"
    class="flex flex-col gap-4"
    :class="{ 'flex-1': compact }"
    :aria-busy="loading"
  >
    <div class="flex flex-col gap-1">
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
