<script setup lang="ts">
import { ArrowDown, ArrowUp, Minus } from 'lucide-vue-next'
import { computed } from 'vue'

import { changeOf } from '@/lib/change'
import { formatNumber, formatPercent } from '@/lib/format'

import Sparkline from './Sparkline.vue'

export interface StatTile {
  label: string
  value: number
  /** The previous period's value; set it to show a delta. */
  previous?: number
  /** `percent` reads `value` as a ratio (0.037 is 3.7%). */
  format?: 'number' | 'percent'
  /** One value per bucket, for a sparkline under the number. */
  series?: number[]
  /** The sparkline's first open bucket, drawn dotted. */
  partialFrom?: number
}

/**
 * A row of KPI tiles: the number, a pill with its change against the
 * previous period and the previous value, and an optional sparkline.
 * Tiles share the row and wrap, two to a row on a phone.
 */
const props = withDefaults(defineProps<{ tiles: StatTile[]; loading?: boolean }>(), {
  loading: false,
})

const pills = {
  up: 'badge-success',
  new: 'badge-success',
  down: 'badge-error',
  flat: 'badge-neutral',
}
const icons = { up: ArrowUp, new: ArrowUp, down: ArrowDown, flat: Minus }

function format(tile: StatTile, value: number): string {
  return tile.format === 'percent' ? formatPercent(value) : formatNumber(value)
}

const rows = computed(() =>
  props.tiles.map((tile) => ({
    label: tile.label,
    value: format(tile, tile.value),
    change: tile.previous === undefined ? undefined : changeOf(tile.value, tile.previous),
    prior: tile.previous === undefined ? undefined : `vs. ${format(tile, tile.previous)} prior`,
    series: tile.series?.length ? tile.series : undefined,
    partialFrom: tile.partialFrom,
  })),
)
</script>

<template>
  <dl class="flex flex-wrap gap-4" :aria-busy="loading">
    <div
      v-for="row in rows"
      :key="row.label"
      class="flex min-w-40 flex-1 basis-40 flex-col gap-1 rounded-box border border-base-300 bg-base-100 p-4"
    >
      <dt class="text-sm text-base-content/70">{{ row.label }}</dt>
      <dd v-if="loading" class="flex flex-col gap-2 pt-1">
        <span class="h-7 w-20 animate-pulse rounded-field bg-base-300"></span>
        <span class="h-4 w-12 animate-pulse rounded-field bg-base-300"></span>
      </dd>
      <template v-else>
        <dd class="flex flex-wrap items-center justify-between gap-2">
          <span class="text-2xl font-semibold tabular-nums">{{ row.value }}</span>
          <span
            v-if="row.change"
            class="badge gap-0.5 badge-soft badge-sm tabular-nums"
            :class="pills[row.change.direction]"
            data-part="change"
          >
            <component :is="icons[row.change.direction]" class="size-3" aria-hidden="true" />
            <span aria-hidden="true">{{ row.change.text }}</span>
            <span class="sr-only">{{ row.change.spoken }}</span>
          </span>
        </dd>
        <dd v-if="row.prior" class="text-sm text-base-content/70 tabular-nums">
          {{ row.prior }}
        </dd>
        <dd v-if="row.series" class="mt-auto pt-2">
          <Sparkline :values="row.series" :partial-from="row.partialFrom" />
        </dd>
      </template>
    </div>
  </dl>
</template>
