<script setup lang="ts">
import { watch } from 'vue'
import { useRouter } from 'vue-router'

import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { useFeatures } from '@/composables/useFeatures'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { projectNav, projectPath } from '@/modules'

const props = defineProps<{ workspaceId: string; projectId: string }>()
const router = useRouter()
const features = useFeatures(() => props.workspaceId)
const isAdmin = useIsAdmin(() => props.workspaceId)

// The project's first page this user can see, in a feature that is on.
// One-shot, like WorkspaceHome: a re-run during the leave transition would
// undo the user's next navigation.
const stop = watch(
  features.loaded,
  (loaded) => {
    if (!loaded) return
    stop()
    const first = projectNav.find(
      (item) => (!item.adminOnly || isAdmin.value) && features.isOn(item.module),
    )
    void router.replace(
      first
        ? projectPath(props.workspaceId, props.projectId, first.path)
        : `/w/${props.workspaceId}/projects`,
    )
  },
  { immediate: true },
)
</script>

<template>
  <SkeletonRows />
</template>
