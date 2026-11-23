<script setup lang="ts">
import { computed } from 'vue'

import { useIsAdmin } from '@/composables/useIsAdmin'
import { modules } from '@/modules'

const props = defineProps<{
  workspaceId: string
  projectId: string
  /** The entity kind, such as `forms.form`. */
  kind: string
  entityId: string
}>()

const isAdmin = useIsAdmin(() => props.workspaceId)
// `modules` is read here, never at module load: modules/index.ts imports the
// pages that render this component, so the import is a cycle.
const sections = computed(() =>
  modules
    .flatMap((module) => module.entitySections ?? [])
    .filter((section) => !section.adminOnly || isAdmin.value),
)
</script>

<template>
  <component
    :is="section.component"
    v-for="section in sections"
    :key="section.key"
    :workspace-id="workspaceId"
    :project-id="projectId"
    :kind="kind"
    :entity-id="entityId"
  />
</template>
