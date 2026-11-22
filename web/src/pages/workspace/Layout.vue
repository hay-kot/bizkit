<script setup lang="ts">
import { computed } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import Loading from '@/components/Loading.vue'
import { useMe } from '@/queries/auth'
import { useWorkspace } from '@/queries/workspaces'

const props = defineProps<{ workspaceId: string }>()
const { data: workspace, error, isPending } = useWorkspace(() => props.workspaceId)
const { data: me } = useMe()
const isAdmin = computed(
  () => me.value?.memberships.find((m) => m.workspace.id === props.workspaceId)?.role === 'admin',
)
const tabs = computed(() => [
  { to: 'projects', label: 'Projects' },
  { to: 'members', label: 'Members' },
  ...(isAdmin.value
    ? [
        { to: 'invitations', label: 'Invitations' },
        { to: 'api-keys', label: 'API keys' },
        { to: 'settings', label: 'Settings' },
      ]
    : []),
])
</script>

<template>
  <Loading v-if="isPending" />
  <ErrorAlert v-else-if="error" :error="error" />
  <div v-else-if="workspace">
    <div class="mb-4 flex flex-wrap items-baseline gap-2">
      <RouterLink to="/" class="link text-sm link-hover">Workspaces</RouterLink>
      <span aria-hidden="true">/</span>
      <span class="text-xl font-semibold" data-testid="workspace-name">{{ workspace.name }}</span>
    </div>
    <nav class="tabs-box mb-6 tabs w-fit" aria-label="Workspace sections">
      <RouterLink
        v-for="tab in tabs"
        :key="tab.to"
        :to="`/w/${workspaceId}/${tab.to}`"
        class="tab"
        active-class="tab-active"
      >
        {{ tab.label }}
      </RouterLink>
    </nav>
    <RouterView :workspace-id="workspaceId" :is-admin="isAdmin" />
  </div>
</template>
