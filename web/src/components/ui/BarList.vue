<script setup lang="ts">
import { computed } from 'vue'
import type { RouteLocationRaw } from 'vue-router'

import { formatNumber } from '@/lib/format'

import SkeletonRows from './SkeletonRows.vue'

export interface BarListItem {
  key: string
  label: string
  value: number
  secondary?: number
  to?: RouteLocationRaw
}

/**
 * A ranked list with a share bar behind each row, scaled to the top value.
 * Shows the first `limit` items and a "Show all" button (`show-all`) when
 * there are more.
 */
const props = withDefaults(
  defineProps<{
    items: BarListItem[]
    valueLabel: string
    secondaryLabel?: string
    limit?: number
    loading?: boolean
    empty?: string
  }>(),
  { secondaryLabel: undefined, limit: 10, loading: false, empty: 'No data for this range' },
)
defineEmits<{ 'show-all': [] }>()

const shown = computed(() => props.items.slice(0, props.limit))
const max = computed(() => Math.max(1, ...props.items.map((item) => item.value)))
</script>

<template>
  <SkeletonRows v-if="loading" :rows="5" />
  <p v-else-if="!items.length" class="py-6 text-center text-sm text-base-content/70">
    {{ empty }}
  </p>
  <div v-else class="flex flex-col gap-1">
    <div
      class="flex items-center gap-4 px-2 text-xs font-medium text-base-content/70"
      aria-hidden="true"
    >
      <span class="flex-1"></span>
      <span v-if="secondaryLabel" class="w-20 text-right">{{ secondaryLabel }}</span>
      <span class="w-20 text-right">{{ valueLabel }}</span>
    </div>
    <ol class="flex flex-col gap-1">
      <li v-for="item in shown" :key="item.key" class="flex items-center gap-4 text-sm">
        <div class="relative flex h-8 min-w-0 flex-1 items-center px-2">
          <!-- The progress element draws the bar from its value, so no width style is needed. -->
          <progress
            class="progress absolute inset-0 h-full rounded-field bg-transparent text-primary/15"
            :value="item.value"
            :max="max"
            aria-hidden="true"
          ></progress>
          <RouterLink
            v-if="item.to"
            :to="item.to"
            class="relative truncate link-hover"
            :title="item.label"
            >{{ item.label }}</RouterLink
          >
          <span v-else class="relative truncate" :title="item.label">{{ item.label }}</span>
        </div>
        <span v-if="secondaryLabel" class="w-20 text-right text-base-content/70 tabular-nums">
          <span class="sr-only">{{ secondaryLabel }}: </span
          >{{ item.secondary === undefined ? '' : formatNumber(item.secondary) }}
        </span>
        <span class="w-20 text-right font-medium tabular-nums">
          <span class="sr-only">{{ valueLabel }}: </span>{{ formatNumber(item.value) }}
        </span>
      </li>
    </ol>
    <button
      v-if="items.length > limit"
      type="button"
      class="btn self-center btn-ghost btn-sm"
      @click="$emit('show-all')"
    >
      Show all
    </button>
  </div>
</template>
