<script setup lang="ts">
import { computed, useAttrs } from 'vue'

import TabLayout from '@/components/ui/TabLayout.vue'

/** The mailing tabs' parent route: header and tabs, rendered once. */
defineOptions({ inheritAttrs: false })
const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const attrs = useAttrs()

const tabs = computed(() => {
  const base = `/w/${props.workspaceId}/p/${props.projectId}/mailing`
  return [
    { label: 'Lists', to: base, exact: true },
    { label: 'Broadcasts', to: `${base}/broadcasts` },
    { label: 'Sequences', to: `${base}/sequences` },
    { label: 'Suppressions', to: `${base}/suppressions` },
  ]
})
const childProps = computed(() => ({
  ...attrs,
  workspaceId: props.workspaceId,
  projectName: props.projectName,
}))
</script>

<template>
  <TabLayout :section="projectName" :tabs="tabs" label="Mailing pages" :child-props="childProps" />
</template>
