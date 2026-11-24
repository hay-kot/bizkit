<script setup lang="ts">
import { computed } from 'vue'

import Tabs from '@/components/ui/Tabs.vue'
import { useIsAdmin } from '@/composables/useIsAdmin'

const props = defineProps<{ workspaceId: string; projectId: string }>()
const isAdmin = useIsAdmin(() => props.workspaceId)

const tabs = computed(() => {
  const base = `/w/${props.workspaceId}/p/${props.projectId}/analytics`
  return [
    { label: 'Overview', to: base, exact: true },
    { label: 'Events', to: `${base}/events` },
    { label: 'Insights', to: `${base}/insights` },
    { label: 'Dashboards', to: `${base}/dashboards` },
    { label: 'Funnels', to: `${base}/funnels` },
    { label: 'People', to: `${base}/people` },
    { label: 'Retention', to: `${base}/retention` },
    ...(isAdmin.value ? [{ label: 'Settings', to: `${base}/settings` }] : []),
  ]
})
</script>

<template>
  <Tabs :tabs="tabs" label="Analytics pages" />
</template>
