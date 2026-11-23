<script setup lang="ts">
import { Fingerprint } from 'lucide-vue-next'

import EmptyState from '@/components/EmptyState.vue'
import { useIsAdmin } from '@/composables/useIsAdmin'

/** The empty state of a report that needs stored ids on a cookieless project. */
const props = defineProps<{ workspaceId: string; projectId: string; what: string }>()
const isAdmin = useIsAdmin(() => props.workspaceId)
</script>

<template>
  <EmptyState
    :icon="Fingerprint"
    :title="`${what} needs stored ids`"
    message="This project is cookieless, so visitors can't be followed from one day to the next. Choose another visitor identity mode to see who comes back."
  >
    <RouterLink
      v-if="isAdmin"
      :to="`/w/${workspaceId}/p/${projectId}/analytics/settings`"
      class="btn btn-sm"
    >
      Change visitor identity
    </RouterLink>
  </EmptyState>
</template>
