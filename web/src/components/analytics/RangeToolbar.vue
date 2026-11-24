<script setup lang="ts">
import { GitCompareArrows } from 'lucide-vue-next'

import TimeRangePicker from '@/components/ui/TimeRangePicker.vue'
import type { DateRange } from '@/lib/range'

/**
 * The row above a report: the time range, the compare toggle inside the
 * range's group when the page offers one, and page filters in the default
 * slot.
 */
defineProps<{ today: string; withCompare?: boolean }>()
const range = defineModel<DateRange>('range', { required: true })
const compare = defineModel<boolean>('compare', { default: false })
</script>

<template>
  <div class="flex flex-wrap items-center gap-2">
    <TimeRangePicker v-if="withCompare" :range="range" :today="today" @apply="range = $event">
      <button
        type="button"
        class="flex size-8 shrink-0 items-center justify-center rounded-selector text-base-content/70 transition-colors hover:bg-base-content/10 hover:text-base-content focus-visible:outline-2 focus-visible:outline-primary aria-pressed:bg-primary/10 aria-pressed:text-primary"
        :aria-pressed="compare"
        aria-label="Compare to previous period"
        title="Compare to previous period"
        @click="compare = !compare"
      >
        <GitCompareArrows class="size-4" aria-hidden="true" />
      </button>
    </TimeRangePicker>
    <TimeRangePicker v-else :range="range" :today="today" @apply="range = $event" />
    <slot />
  </div>
</template>
