<script setup lang="ts">
import { computed, useAttrs } from 'vue'

import TabLayout from '@/components/ui/TabLayout.vue'
import { useIsAdmin } from '@/composables/useIsAdmin'

/** The analytics pages' parent route: header and tabs, rendered once. */
defineOptions({ inheritAttrs: false })
const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const attrs = useAttrs()
const isAdmin = useIsAdmin(() => props.workspaceId)

const tabs = computed(() => {
  const base = `/w/${props.workspaceId}/p/${props.projectId}/analytics`
  return [
    { label: 'Overview', to: base, exact: true },
    { label: 'Events', to: `${base}/events` },
    { label: 'Insights', to: `${base}/insights`, also: [`${base}/dashboards`] },
    { label: 'Funnels', to: `${base}/funnels` },
    { label: 'People', to: `${base}/people` },
    { label: 'Retention', to: `${base}/retention` },
    ...(isAdmin.value ? [{ label: 'Settings', to: `${base}/settings` }] : []),
  ]
})
const childProps = computed(() => ({
  ...attrs,
  workspaceId: props.workspaceId,
  projectName: props.projectName,
}))
</script>

<template>
  <TabLayout :tabs="tabs" label="Analytics pages" :child-props="childProps" />
</template>
