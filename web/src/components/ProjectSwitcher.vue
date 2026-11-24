<script setup lang="ts">
import { Check, ChevronsUpDown, FolderKanban, List } from 'lucide-vue-next'
import { computed } from 'vue'
import { useRoute } from 'vue-router'

import Avatar from '@/components/ui/Avatar.vue'
import Menu from '@/components/ui/Menu.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import { projectNav, projectPath } from '@/modules'
import type { Project } from '@/queries/projects'

const props = defineProps<{ workspaceId: string; projects: Project[]; current?: Project }>()
const route = useRoute()

// Switching keeps you on the same module page, but not on a record inside it:
// a form id from one project means nothing in another.
const section = computed(() => {
  const match = /\/p\/[^/]+\/([^/]+)/.exec(route.path)
  return projectNav.some((item) => item.path === match?.[1]) ? match?.[1] : undefined
})
</script>

<template>
  <Menu label="Switch project" class="w-full">
    <template #trigger="{ open, toggle, controls }">
      <button
        type="button"
        class="flex h-12 w-full items-center gap-2.5 rounded-field border border-field-border bg-base-100 px-2.5 text-left transition-colors hover:border-base-content/60 hover:bg-raised"
        :aria-expanded="open"
        :aria-controls="controls"
        aria-haspopup="menu"
        @click="toggle"
      >
        <Avatar v-if="current" :name="current.name" size="sm" />
        <FolderKanban v-else class="size-6 shrink-0 p-0.5" aria-hidden="true" />
        <span class="flex min-w-0 flex-1 flex-col leading-tight">
          <span class="text-xs text-base-content/70">Project</span>
          <span class="truncate text-sm font-semibold" data-testid="project-name">{{
            current?.name ?? 'Choose a project'
          }}</span>
        </span>
        <span class="sr-only">Switch project</span>
        <ChevronsUpDown class="size-4 shrink-0 text-base-content/70" aria-hidden="true" />
      </button>
    </template>
    <div class="px-3 pt-2 pb-1 text-xs font-medium text-base-content/70">Projects</div>
    <MenuItem v-for="p in projects" :key="p.id" :to="projectPath(props.workspaceId, p.id, section)">
      <span class="flex-1 truncate">{{ p.name }}</span>
      <Check v-if="p.id === current?.id" class="size-4" aria-hidden="true" />
    </MenuItem>
    <div class="my-1 border-t border-base-300"></div>
    <MenuItem :to="`/w/${workspaceId}/projects`"
      ><List class="size-4" aria-hidden="true" />Manage projects</MenuItem
    >
  </Menu>
</template>
