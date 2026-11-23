<script setup lang="ts">
import { computed } from 'vue'

import { useIsAdmin } from '@/composables/useIsAdmin'
import { useEntitySections } from '@/modules/entitySections'

const props = defineProps<{
  workspaceId: string
  projectId: string
  /** The entity kind, such as `forms.form`. */
  kind: string
  entityId: string
}>()

const isAdmin = useIsAdmin(() => props.workspaceId)
const all = useEntitySections()
const sections = computed(() => all.filter((section) => !section.adminOnly || isAdmin.value))
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
