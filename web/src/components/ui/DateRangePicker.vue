<script setup lang="ts">
import { type DateRange, presetRange, RANGE_PRESETS, type RangePreset } from '@/lib/range'

import Select from './Select.vue'

/**
 * A preset range, or two dates for `custom`. `today` comes from the caller
 * (in UTC, like the range) so the presets stay testable.
 */
const props = defineProps<{ today: string }>()
const model = defineModel<DateRange>({ required: true })

function pickPreset(preset: RangePreset) {
  model.value =
    preset === 'custom'
      ? { ...model.value, preset }
      : { preset, ...presetRange(preset, props.today) }
}

function setDay(end: 'from' | 'to', event: Event) {
  const day = (event.target as HTMLInputElement).value
  if (!day) return
  const next = { ...model.value, preset: 'custom' as const, [end]: day }
  // Keep the range the right way round when one end jumps past the other.
  if (next.from > next.to) {
    if (end === 'from') next.to = day
    else next.from = day
  }
  model.value = next
}
</script>

<template>
  <div class="flex flex-wrap items-center gap-2">
    <div class="w-44">
      <Select
        :model-value="model.preset"
        :options="RANGE_PRESETS"
        label="Date range"
        size="toolbar"
        @update:model-value="pickPreset"
      />
    </div>
    <template v-if="model.preset === 'custom'">
      <input
        type="date"
        class="input h-9 w-auto"
        aria-label="From"
        :value="model.from"
        :max="today"
        @change="setDay('from', $event)"
      />
      <span class="text-sm text-base-content/60" aria-hidden="true">to</span>
      <input
        type="date"
        class="input h-9 w-auto"
        aria-label="To"
        :value="model.to"
        :max="today"
        @change="setDay('to', $event)"
      />
    </template>
  </div>
</template>
