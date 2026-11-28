<script setup lang="ts">
import type { HandleBindings } from '@/components/ui/DashboardGrid.vue'
import DashboardTile from '@/components/ui/DashboardTile.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import type { TileWidth } from '@/components/ui/dashboardGrid'
import type { DayRange } from '@/lib/range'
import { type Insight, useInsightResult } from '@/queries/analytics'

import { describeQuery } from './insightDraft'
import InsightResultView from './InsightResultView.vue'
import { useInsightTrend } from './insightTrend'

/**
 * One dashboard tile: a saved insight over the dashboard's range, drawn
 * by its display. Each tile loads and fails on its own; a number tile
 * also loads the previous period for its change. The menu opens the
 * insight or asks the page to remove the tile (`remove`).
 */
const props = defineProps<{
  workspaceId: string
  insight: Insight
  range: DayRange
  to: string
  editing: boolean
  handle: HandleBindings
  locked?: boolean
}>()
const width = defineModel<TileWidth>('width', { required: true })
defineEmits<{ remove: [] }>()
const result = useInsightResult(
  () => props.workspaceId,
  () => props.insight.project_id,
  () => props.range,
  () => props.insight.query,
)
const { previous } = useInsightTrend(
  () => props.workspaceId,
  () => props.insight.project_id,
  () => props.range,
  () => props.insight.query,
  () => props.insight.display === 'number' && props.insight.query.kind === 'events',
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
    :locked="locked"
    @remove="$emit('remove')"
  >
    <InsightResultView
      :result="result.data.value"
      :loading="result.isLoading.value"
      :error="result.error.value"
      :display="insight.display"
      :previous="previous"
      compact
    />
    <template #menu>
      <MenuItem :to="to">Open insight</MenuItem>
      <MenuItem danger @select="$emit('remove')">Remove from dashboard</MenuItem>
    </template>
  </DashboardTile>
</template>
