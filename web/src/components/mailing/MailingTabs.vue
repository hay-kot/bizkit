<script setup lang="ts">
import { computed } from 'vue'

import Tabs from '@/components/ui/Tabs.vue'

const props = defineProps<{ workspaceId: string; projectId: string }>()

const tabs = computed(() => {
  const base = `/w/${props.workspaceId}/p/${props.projectId}/mailing`
  return [
    { label: 'Lists', to: base, exact: true },
    { label: 'Broadcasts', to: `${base}/broadcasts` },
    { label: 'Sequences', to: `${base}/sequences` },
  ]
})
</script>

<template>
  <Tabs :tabs="tabs" label="Mailing pages" />
</template>
