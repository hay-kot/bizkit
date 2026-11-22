<script setup lang="ts">
import { Check, ChevronsUpDown, FolderKanban, List } from 'lucide-vue-next'
import { computed } from 'vue'
import { useRoute } from 'vue-router'

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
  <Menu label="Switch project">
    <template #trigger="{ open, toggle, controls }">
      <button
        type="button"
        class="flex w-full items-center gap-2.5 rounded-field px-2.5 py-2 text-left text-sm transition-colors hover:bg-base-300"
        :aria-expanded="open"
        :aria-controls="controls"
        aria-haspopup="menu"
        @click="toggle"
      >
        <FolderKanban class="size-4 shrink-0" aria-hidden="true" />
        <span class="flex-1 truncate font-medium" data-testid="project-name">{{
          current?.name ?? 'Choose a project'
        }}</span>
        <span class="sr-only">Switch project</span>
        <ChevronsUpDown class="size-4 shrink-0 text-base-content/70" aria-hidden="true" />
      </button>
    </template>
    <div class="px-2 py-1.5 text-xs font-medium text-base-content/70">Projects</div>
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
