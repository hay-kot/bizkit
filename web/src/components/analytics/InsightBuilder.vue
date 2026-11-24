<script setup lang="ts">
import { refDebounced } from '@vueuse/core'
import { computed } from 'vue'

import type { DateRange } from '@/lib/range'
import { useEventStats, useInsightResult } from '@/queries/analytics'

import InsightControls from './InsightControls.vue'
import { type InsightDraft, isRunnable, queryOf } from './insightDraft'
import InsightResultView from './InsightResultView.vue'
import RangeToolbar from './RangeToolbar.vue'

/**
 * The insight builder: the range, the controls and the live result. The
 * query waits for typing to pause before it runs.
 */
const props = defineProps<{ workspaceId: string; projectId: string; today: string }>()
const range = defineModel<DateRange>('range', { required: true })
const draft = defineModel<InsightDraft>('draft', { required: true })

const TYPING_PAUSE_MS = 300
// The query and whether it can run come from the same settled draft, so
// a half-made draft never reaches the server.
const settledDraft = refDebounced(draft, TYPING_PAUSE_MS)
const settled = computed(() => queryOf(settledDraft.value))
const runnable = computed(() => isRunnable(settledDraft.value))
const result = useInsightResult(
  () => props.workspaceId,
  () => props.projectId,
  range,
  settled,
  runnable,
)
const names = useEventStats(
  () => props.workspaceId,
  () => props.projectId,
  range,
  () => ({ name: null, props: [] }),
)
const eventNames = computed(() => (names.data.value?.names ?? []).map((n) => n.name))
</script>

<template>
  <RangeToolbar v-model:range="range" :today="today" />
  <InsightControls v-model="draft" :event-names="eventNames" />
  <p v-if="!isRunnable(draft)" class="py-6 text-center text-sm text-base-content/70">
    Pick an event to see its numbers.
  </p>
  <InsightResultView
    v-else
    :result="result.data.value"
    :loading="result.isLoading.value"
    :error="result.error.value"
  />
</template>
