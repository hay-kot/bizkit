<script setup lang="ts">
import { LayoutDashboard, Plus } from 'lucide-vue-next'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import { useCreateIntent } from '@/composables/useCreateIntent'
import { formatRelative } from '@/lib/format'
import { useCreateDashboard, useDashboards } from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const router = useRouter()
const {
  data: dashboards,
  isPending,
  error,
} = useDashboards(
  () => props.workspaceId,
  () => props.projectId,
)
const base = computed(() => `/w/${props.workspaceId}/p/${props.projectId}/analytics/dashboards`)
const create = useCreateDashboard()
const panelOpen = ref(false)
const name = ref('')
useCreateIntent(openPanel)

const columns = [
  { key: 'name', label: 'Name' },
  { key: 'tiles', label: 'Insights' },
  { key: 'updated', label: 'Updated' },
]

function openPanel() {
  name.value = ''
  create.reset()
  panelOpen.value = true
}

async function onCreate() {
  const board = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { project_id: props.projectId, name: name.value },
  })
  panelOpen.value = false
  await router.push(`${base.value}/${board.id}`)
}
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    title="Dashboards"
    description="Saved insights side by side, over one date range."
  >
    <template #actions>
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New dashboard
      </button>
    </template>
    <DataTable
      :columns="columns"
      :rows="dashboards?.items"
      :row-key="(d) => d.id"
      :row-to="(r) => `${base}/${r.id}`"
      :loading="isPending"
      :error="error"
      :empty="{
        icon: LayoutDashboard,
        title: 'No dashboards yet',
        message: 'A dashboard shows saved insights two to a row.',
      }"
    >
      <template #cell-name="{ row }">
        {{ row.name }}
      </template>
      <template #cell-tiles="{ row }">
        <span class="text-sm tabular-nums">{{ row.insight_ids.length }}</span>
      </template>
      <template #cell-updated="{ row }">
        <span class="text-sm">{{ formatRelative(row.updated_at) }}</span>
      </template>
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">
          New dashboard
        </button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      variant="dialog"
      title="New dashboard"
      submit-label="Create dashboard"
      :pending="create.isLoading.value"
      :error="create.error.value"
      @close="panelOpen = false"
      @submit="onCreate"
    >
      <Field label="Name">
        <input v-model="name" name="dashboard-name" class="input w-full" required maxlength="100" />
      </Field>
    </FormPanel>
  </AnalyticsShell>
</template>
