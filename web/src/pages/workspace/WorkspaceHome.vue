<script setup lang="ts">
import { watch } from 'vue'
import { useRouter } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { useCurrentProject } from '@/composables/useCurrentProject'
import { useFeatures } from '@/composables/useFeatures'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { projectNav, projectPath } from '@/modules'

const props = defineProps<{ workspaceId: string }>()
const router = useRouter()
const { project, isPending, error } = useCurrentProject(() => props.workspaceId)
const features = useFeatures(() => props.workspaceId)
const isAdmin = useIsAdmin(() => props.workspaceId)

// One-shot: this page stays mounted through its leave transition, and a
// re-run then would undo the user's next navigation.
const stop = watch(
  [isPending, error, features.loaded],
  () => {
    if (isPending.value || error.value || !features.loaded.value) return
    stop()
    // The first project page this user can see in a feature that is on.
    const first = projectNav.find(
      (item) => (!item.adminOnly || isAdmin.value) && features.isOn(item.module),
    )
    void router.replace(
      project.value && first
        ? projectPath(props.workspaceId, project.value.id, first.path)
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
