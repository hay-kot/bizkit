<script setup lang="ts">
import { Lightbulb, Plus } from 'lucide-vue-next'
import { computed } from 'vue'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import { describeQuery } from '@/components/analytics/insightDraft'
import DataTable from '@/components/ui/DataTable.vue'
import { formatRelative } from '@/lib/format'
import { useInsights } from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const {
  data: insights,
  isPending,
  error,
} = useInsights(
  () => props.workspaceId,
  () => props.projectId,
)
const base = computed(() => `/w/${props.workspaceId}/p/${props.projectId}/analytics/insights`)

const columns = [
  { key: 'name', label: 'Name' },
  { key: 'query', label: 'Counts' },
  { key: 'dashboards', label: 'Dashboards' },
  { key: 'updated', label: 'Updated' },
]
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    title="Insights"
    description="Saved questions about your events and people."
  >
    <template #actions>
      <RouterLink :to="`${base}/new`" class="btn gap-1.5 btn-primary btn-sm">
        <Plus class="size-4" aria-hidden="true" />New insight
      </RouterLink>
    </template>
    <DataTable
      :columns="columns"
      :rows="insights?.items"
      :row-key="(i) => i.id"
      :loading="isPending"
      :error="error"
      :empty="{
        icon: Lightbulb,
        title: 'No insights yet',
        message: 'An insight counts events or people, and you can pin it to a dashboard.',
      }"
    >
      <template #cell-name="{ row }">
        <RouterLink :to="`${base}/${row.id}`" class="link font-medium link-hover">
          {{ row.name }}
        </RouterLink>
      </template>
      <template #cell-query="{ row }">
        <span class="line-clamp-1 text-sm">{{ describeQuery(row.query) }}</span>
      </template>
      <template #cell-dashboards="{ row }">
        <span class="line-clamp-1 text-sm">{{ row.dashboards.map((d) => d.name).join(', ') }}</span>
      </template>
      <template #cell-updated="{ row }">
        <span class="text-sm">{{ formatRelative(row.updated_at) }}</span>
      </template>
      <template #empty-action>
        <RouterLink :to="`${base}/new`" class="btn btn-primary btn-sm">New insight</RouterLink>
      </template>
    </DataTable>
  </AnalyticsShell>
</template>
