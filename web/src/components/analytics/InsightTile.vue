<script setup lang="ts">
import type { HandleBindings } from '@/components/ui/DashboardGrid.vue'
import DashboardTile from '@/components/ui/DashboardTile.vue'
import type { TileWidth } from '@/components/ui/dashboardGrid'
import type { DayRange } from '@/lib/range'
import { type Insight, useInsightResult } from '@/queries/analytics'

import { describeQuery } from './insightDraft'
import InsightResultView from './InsightResultView.vue'

/**
 * One dashboard tile: a saved insight over the dashboard's range. Each
 * tile loads and fails on its own.
 */
const props = defineProps<{
  workspaceId: string
  insight: Insight
  range: DayRange
  to: string
  editing: boolean
  handle: HandleBindings
}>()
const width = defineModel<TileWidth>('width', { required: true })
defineEmits<{ remove: [] }>()
const result = useInsightResult(
  () => props.workspaceId,
  () => props.insight.project_id,
  () => props.range,
  () => props.insight.query,
)
</script>

<template>
  <DashboardTile
    v-model:width="width"
    :title="insight.name"
    :description="describeQuery(insight.query)"
    :to="to"
    :editing="editing"
    :handle="handle"
    @remove="$emit('remove')"
  >
    <InsightResultView
      :result="result.data.value"
      :loading="result.isLoading.value"
      :error="result.error.value"
      compact
    />
  </DashboardTile>
</template>
