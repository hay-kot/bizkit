<script setup lang="ts">
import { Code, Plus } from 'lucide-vue-next'
import { computed, ref } from 'vue'

import DataTable from '@/components/ui/DataTable.vue'
import PageSection from '@/components/ui/PageSection.vue'
import { useHooks, useScripts } from '@/queries/extensions'

import NewScriptPanel from './NewScriptPanel.vue'
import { runsForText } from './targets'

/**
 * The scripts that run for one entity, on that entity's page: those that
 * name it and those for every entity of its kind. Renders nothing when no
 * hook point targets the kind.
 */
const props = defineProps<{
  workspaceId: string
  projectId: string
  kind: string
  entityId: string
}>()

const { data: hooks } = useHooks(() => props.workspaceId)
const targetKind = computed(
  () => hooks.value?.items.find((hook) => hook.target?.id === props.kind)?.target,
)
const {
  data: scripts,
  isPending,
  error,
} = useScripts(
  () => props.workspaceId,
  () => props.projectId,
  () => props.entityId,
)
const panelOpen = ref(false)

const columns = [
  { key: 'name', label: 'Name' },
  { key: 'hook', label: 'Hook' },
  { key: 'target', label: 'Runs for' },
]

function scriptPath(scriptId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/extensions/${scriptId}`
}
</script>

<template>
  <PageSection v-if="targetKind" title="Extensions">
    <template #description>
      Scripts that run for this {{ targetKind.noun }}: those picked for it and those for all
      {{ targetKind.plural }}.
    </template>
    <template #actions>
      <button type="button" class="btn gap-1.5 btn-sm" @click="panelOpen = true">
        <Plus class="size-4" aria-hidden="true" />New script
      </button>
    </template>
    <DataTable
      :columns="columns"
      :rows="scripts?.items"
      :row-key="(s) => s.id"
      :row-to="(r) => scriptPath(r.id)"
      :loading="isPending"
      :error="error"
      label="Extensions"
      :empty="{
        icon: Code,
        title: 'No scripts',
        message: `Add one to run your own code for this ${targetKind.noun}.`,
      }"
    >
      <template #cell-name="{ row }">
        {{ row.name }}
        <span v-if="!row.enabled" class="ms-2 badge badge-ghost badge-sm">Disabled</span>
      </template>
      <template #cell-hook="{ row }">
        <code class="text-xs">{{ row.hook }}</code>
        <span class="ms-2 badge badge-outline badge-sm">{{ row.sync ? 'sync' : 'async' }}</span>
      </template>
      <template #cell-target="{ row }"
        ><span class="text-sm" :class="{ 'text-base-content/70': row.targets.length === 0 }">{{
          runsForText(row.targets, targetKind)
        }}</span></template
      >
    </DataTable>
    <NewScriptPanel
      :open="panelOpen"
      :workspace-id="workspaceId"
      :project-id="projectId"
      :target="{ kind, id: entityId }"
      @close="panelOpen = false"
    />
  </PageSection>
</template>
