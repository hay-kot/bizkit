<script setup lang="ts">
import { Filter, Plus } from 'lucide-vue-next'
import { ref } from 'vue'
import { useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import FunnelForm from '@/components/analytics/FunnelForm.vue'
import {
  describeStep,
  emptyFunnel,
  type FunnelDraft,
  funnelBody,
} from '@/components/analytics/funnelDraft'
import DataTable from '@/components/ui/DataTable.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import { useCreateIntent } from '@/composables/useCreateIntent'
import { formatRelative } from '@/lib/format'
import { useCreateFunnel, useFunnels } from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const router = useRouter()
const {
  data: funnels,
  isPending,
  error,
} = useFunnels(
  () => props.workspaceId,
  () => props.projectId,
)
const create = useCreateFunnel()
const panelOpen = ref(false)
const draft = ref<FunnelDraft>(emptyFunnel())
useCreateIntent(openPanel)

const columns = [
  { key: 'name', label: 'Name' },
  { key: 'steps', label: 'Steps' },
  { key: 'window', label: 'Window' },
  { key: 'updated', label: 'Updated' },
]

function openPanel() {
  draft.value = emptyFunnel()
  create.reset()
  panelOpen.value = true
}

function funnelPath(funnelId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/analytics/funnels/${funnelId}`
}

async function onCreate() {
  const funnel = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { project_id: props.projectId, ...funnelBody(draft.value) },
  })
  panelOpen.value = false
  await router.push(funnelPath(funnel.id))
}
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    title="Funnels"
    description="Ordered steps people take, and where they drop off."
  >
    <template #actions>
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New funnel
      </button>
    </template>
    <DataTable
      :columns="columns"
      :rows="funnels?.items"
      :row-key="(f) => f.id"
      :row-to="(r) => funnelPath(r.id)"
      :loading="isPending"
      :error="error"
      :empty="{
        icon: Filter,
        title: 'No funnels yet',
        message: 'A funnel shows how many people go from one step to the next.',
      }"
    >
      <template #cell-name="{ row }">
        {{ row.name }}
      </template>
      <template #cell-steps="{ row }">
        <span class="line-clamp-1 text-sm">{{ row.steps.map(describeStep).join(' → ') }}</span>
      </template>
      <template #cell-window="{ row }">
        <span class="text-sm tabular-nums">{{ row.window_days }} days</span>
      </template>
      <template #cell-updated="{ row }">
        <span class="text-sm">{{ formatRelative(row.updated_at) }}</span>
      </template>
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">New funnel</button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      title="New funnel"
      submit-label="Create funnel"
      :pending="create.asyncStatus.value === 'loading'"
      :error="create.error.value"
      @close="panelOpen = false"
      @submit="onCreate"
    >
      <FunnelForm v-model="draft" :workspace-id="workspaceId" :project-id="projectId" />
    </FormPanel>
  </AnalyticsShell>
</template>
