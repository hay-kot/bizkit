<script setup lang="ts">
import { ArrowDown, ArrowUp, Minus } from 'lucide-vue-next'
import { computed } from 'vue'

import { changeOf } from '@/lib/change'
import { formatNumber, formatPercent } from '@/lib/format'

export interface StatTile {
  label: string
  value: number
  /** The previous period's value; set it to show a delta. */
  previous?: number
  /** `percent` reads `value` as a ratio (0.037 is 3.7%). */
  format?: 'number' | 'percent'
}

/** A row of KPI tiles, each with an optional change against the previous period. */
const props = withDefaults(defineProps<{ tiles: StatTile[]; loading?: boolean }>(), {
  loading: false,
})

const tones = {
  up: 'text-success',
  new: 'text-success',
  down: 'text-error',
  flat: 'text-base-content/70',
}
const icons = { up: ArrowUp, new: ArrowUp, down: ArrowDown, flat: Minus }

const rows = computed(() =>
  props.tiles.map((tile) => ({
    label: tile.label,
    value: tile.format === 'percent' ? formatPercent(tile.value) : formatNumber(tile.value),
    change: tile.previous === undefined ? undefined : changeOf(tile.value, tile.previous),
  })),
)
</script>

<template>
  <dl class="grid grid-cols-2 gap-4 lg:grid-cols-4" :aria-busy="loading">
    <div
      v-for="row in rows"
      :key="row.label"
      class="flex flex-col gap-1 rounded-box border border-base-300 bg-base-100 p-4"
    >
      <dt class="text-sm text-base-content/70">{{ row.label }}</dt>
      <dd v-if="loading" class="flex flex-col gap-2 pt-1">
        <span class="h-7 w-20 animate-pulse rounded-field bg-base-300"></span>
        <span class="h-4 w-12 animate-pulse rounded-field bg-base-300"></span>
      </dd>
      <template v-else>
        <dd class="text-2xl font-semibold tabular-nums">{{ row.value }}</dd>
        <dd
          v-if="row.change"
          class="flex items-center gap-1 text-sm tabular-nums"
          :class="tones[row.change.direction]"
        >
          <component :is="icons[row.change.direction]" class="size-3.5" aria-hidden="true" />
          <span aria-hidden="true">{{ row.change.text }}</span>
          <span class="sr-only">{{ row.change.spoken }}</span>
        </dd>
      </template>
    </div>
  </dl>
</template>
