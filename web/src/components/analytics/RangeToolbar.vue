<script setup lang="ts">
import Checkbox from '@/components/ui/Checkbox.vue'
import DateRangePicker from '@/components/ui/DateRangePicker.vue'
import type { DateRange } from '@/lib/range'

/**
 * The row above a report: the date range, the compare switch when the page
 * offers one, and page filters in the default slot.
 */
defineProps<{ today: string; withCompare?: boolean }>()
const range = defineModel<DateRange>('range', { required: true })
const compare = defineModel<boolean>('compare', { default: false })
</script>

<template>
  <div class="flex flex-wrap items-center gap-4">
    <DateRangePicker v-model="range" :today="today" />
    <Checkbox v-if="withCompare" v-model="compare" label="Compare to previous period" />
    <slot />
  </div>
</template>
