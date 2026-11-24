<script setup lang="ts">
import { computed, useAttrs } from 'vue'

import TabLayout from '@/components/ui/TabLayout.vue'

/** The workspace settings tabs' parent route: header and tabs, rendered once. */
defineOptions({ inheritAttrs: false })
const props = defineProps<{ workspaceId: string }>()
const attrs = useAttrs()

const tabs = computed(() => {
  const base = `/w/${props.workspaceId}/settings`
  return [
    { label: 'General', to: base, exact: true },
    { label: 'Email', to: `${base}/email` },
  ]
})
const childProps = computed(() => ({ ...attrs, workspaceId: props.workspaceId }))
</script>

<template>
  <TabLayout :tabs="tabs" label="Settings pages" :child-props="childProps" />
</template>
