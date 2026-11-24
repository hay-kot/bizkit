<script setup lang="ts">
import { Check, ChevronsUpDown, LayoutGrid, Plus } from 'lucide-vue-next'
import { computed } from 'vue'

import Avatar from '@/components/ui/Avatar.vue'
import Menu from '@/components/ui/Menu.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import { useMe } from '@/queries/auth'

const props = defineProps<{ workspaceId: string; name?: string }>()
const { data: me } = useMe()
const current = computed(() =>
  me.value?.memberships.find((m) => m.workspace.id === props.workspaceId),
)
const title = computed(() => props.name ?? current.value?.workspace.name ?? '')
</script>

<template>
  <Menu label="Switch workspace">
    <template #trigger="{ open, toggle, controls }">
      <button
        type="button"
        class="flex w-full items-center gap-2.5 rounded-field p-2 text-left transition-colors hover:bg-raised"
        :aria-expanded="open"
        :aria-controls="controls"
        aria-haspopup="menu"
        @click="toggle"
      >
        <Avatar :name="title" />
        <span class="flex min-w-0 flex-1 flex-col">
          <span class="truncate text-sm font-semibold" data-testid="workspace-name">{{
            title
          }}</span>
          <span class="truncate text-xs text-base-content/70">{{ current?.role ?? '' }}</span>
        </span>
        <span class="sr-only">Switch workspace</span>
        <ChevronsUpDown class="size-4 shrink-0 text-base-content/70" aria-hidden="true" />
      </button>
    </template>
    <div class="px-3 pt-2 pb-1 text-xs font-medium text-base-content/70">Workspaces</div>
    <MenuItem v-for="m in me?.memberships ?? []" :key="m.workspace.id" :to="`/w/${m.workspace.id}`">
      <Avatar :name="m.workspace.name" size="sm" />
      <span class="flex-1 truncate">{{ m.workspace.name }}</span>
      <Check v-if="m.workspace.id === workspaceId" class="size-4" aria-hidden="true" />
    </MenuItem>
    <div class="my-1 border-t border-base-300"></div>
    <MenuItem to="/"><LayoutGrid class="size-4" aria-hidden="true" />All workspaces</MenuItem>
    <MenuItem :to="{ path: '/', query: { new: '1' } }"
      ><Plus class="size-4" aria-hidden="true" />New workspace</MenuItem
    >
  </Menu>
</template>
