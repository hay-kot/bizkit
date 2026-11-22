<script setup lang="ts">
import { watch } from 'vue'
import { useRouter } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { useCurrentProject } from '@/composables/useCurrentProject'
import { projectPath } from '@/modules'

const props = defineProps<{ workspaceId: string }>()
const router = useRouter()
const { project, isPending, error } = useCurrentProject(() => props.workspaceId)

// One-shot: this page stays mounted through its leave transition, and a
// re-run then would undo the user's next navigation.
const stop = watch(
  [isPending, error],
  () => {
    if (isPending.value || error.value) return
    stop()
    void router.replace(
      project.value
        ? projectPath(props.workspaceId, project.value.id)
        : `/w/${props.workspaceId}/projects`,
    )
  },
  { immediate: true },
)
</script>

<template>
  <ErrorAlert v-if="error" :error="error" />
  <SkeletonRows v-else />
</template>
