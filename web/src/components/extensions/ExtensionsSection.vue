<script setup lang="ts">
import { Code, Plus } from 'lucide-vue-next'
import { computed, ref } from 'vue'

import DataTable from '@/components/ui/DataTable.vue'
import { useHooks, useScripts } from '@/queries/extensions'

import NewScriptPanel from './NewScriptPanel.vue'

/**
 * The scripts bound to one entity, on that entity's page. Renders nothing
 * when no hook point targets the entity's kind.
 */
const props = defineProps<{
  workspaceId: string
  projectId: string
  kind: string
  entityId: string
  /** The entity's display name, for the create panel. */
  entityLabel?: string
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
]

function scriptPath(scriptId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/extensions/${scriptId}`
}
</script>

<template>
  <section v-if="targetKind" class="mt-8" aria-labelledby="entity-extensions">
    <div class="mb-3 flex items-center justify-between gap-2">
      <div>
        <h2 id="entity-extensions" class="text-sm font-semibold">Extensions</h2>
        <p class="text-xs text-base-content/70">
          Scripts that run only for this {{ targetKind.noun }}. Project-wide scripts on the
          Extensions page run for all {{ targetKind.plural }}.
        </p>
      </div>
      <button type="button" class="btn gap-1.5 btn-sm" @click="panelOpen = true">
        <Plus class="size-4" aria-hidden="true" />New script
      </button>
    </div>
    <DataTable
      :columns="columns"
      :rows="scripts?.items"
      :row-key="(s) => s.id"
      :loading="isPending"
      :error="error"
      label="Extensions"
      :empty="{
        icon: Code,
        title: 'No scripts',
        message: `Add one to filter or forward this ${targetKind.noun}'s submissions.`,
      }"
    >
      <template #cell-name="{ row }">
        <RouterLink :to="scriptPath(row.id)" class="link font-medium link-hover">
          {{ row.name }}
        </RouterLink>
        <span v-if="!row.enabled" class="ms-2 badge badge-ghost badge-sm">Disabled</span>
      </template>
      <template #cell-hook="{ row }">
        <code class="text-xs">{{ row.hook }}</code>
        <span class="ms-2 badge badge-outline badge-sm">{{ row.sync ? 'sync' : 'async' }}</span>
      </template>
    </DataTable>
    <NewScriptPanel
      :open="panelOpen"
      :workspace-id="workspaceId"
      :project-id="projectId"
      :target="{ kind, id: entityId, label: entityLabel ?? `this ${targetKind.noun}` }"
      @close="panelOpen = false"
    />
  </section>
</template>
