<script setup lang="ts">
import { watchEffect } from 'vue'
import { useRouter } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { useCurrentProject } from '@/composables/useCurrentProject'
import { projectPath } from '@/modules'

const props = defineProps<{ workspaceId: string }>()
const router = useRouter()
const { project, isPending, error } = useCurrentProject(() => props.workspaceId)

watchEffect(() => {
  if (isPending.value || error.value) return
  void router.replace(
    project.value
      ? projectPath(props.workspaceId, project.value.id)
      : `/w/${props.workspaceId}/projects`,
  )
})
</script>

<template>
  <ErrorAlert v-if="error" :error="error" />
  <SkeletonRows v-else />
</template>
