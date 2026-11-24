<script setup lang="ts">
import { ArrowDown, ArrowUp, Ellipsis, X } from 'lucide-vue-next'
import { useId } from 'vue'

import Menu from '@/components/ui/Menu.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
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
  first: boolean
  last: boolean
}>()
defineEmits<{ up: []; down: []; remove: [] }>()
const headingId = useId()
const result = useInsightResult(
  () => props.workspaceId,
  () => props.insight.project_id,
  () => props.range,
  () => props.insight.query,
)
</script>

<template>
  <section
    class="flex flex-col gap-4 rounded-box border border-base-300 bg-base-100 p-4"
    :aria-labelledby="headingId"
  >
    <div class="flex items-start justify-between gap-2">
      <div class="flex min-w-0 flex-col">
        <RouterLink :id="headingId" :to="to" class="link truncate font-medium">
          {{ insight.name }}
        </RouterLink>
        <span class="truncate text-xs text-base-content/70">{{
          describeQuery(insight.query)
        }}</span>
      </div>
      <Menu :label="`${insight.name} tile`" align="end">
        <template #trigger="{ open, toggle, controls }">
          <button
            type="button"
            class="btn btn-square btn-ghost btn-sm"
            :aria-label="`${insight.name} tile actions`"
            aria-haspopup="menu"
            :aria-expanded="open"
            :aria-controls="controls"
            @click="toggle"
          >
            <Ellipsis class="size-4" aria-hidden="true" />
          </button>
        </template>
        <MenuItem v-if="!first" @select="$emit('up')">
          <ArrowUp class="size-4" aria-hidden="true" />Move up
        </MenuItem>
        <MenuItem v-if="!last" @select="$emit('down')">
          <ArrowDown class="size-4" aria-hidden="true" />Move down
        </MenuItem>
        <MenuItem danger @select="$emit('remove')">
          <X class="size-4" aria-hidden="true" />Remove from dashboard
        </MenuItem>
      </Menu>
    </div>
    <InsightResultView
      :result="result.data.value"
      :loading="result.isLoading.value"
      :error="result.error.value"
      compact
    />
  </section>
</template>
