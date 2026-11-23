<script setup lang="ts">
import { FolderX, Menu as MenuIcon, X } from 'lucide-vue-next'
import { ref, watch } from 'vue'
import { useRoute } from 'vue-router'

import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import ProjectSwitcher from '@/components/ProjectSwitcher.vue'
import SidebarLink from '@/components/SidebarLink.vue'
import UserMenu from '@/components/UserMenu.vue'
import WorkspaceSwitcher from '@/components/WorkspaceSwitcher.vue'
import { useCurrentProject } from '@/composables/useCurrentProject'
import { projectNav, projectPath, workspaceNav } from '@/modules'
import type { NavItem } from '@/modules/types'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { useWorkspace } from '@/queries/workspaces'

const props = defineProps<{ workspaceId: string }>()
const route = useRoute()
const { data: workspace, error } = useWorkspace(() => props.workspaceId)
const isAdmin = useIsAdmin(() => props.workspaceId)
const visible = (items: NavItem[]) => items.filter((item) => !item.adminOnly || isAdmin.value)
const {
  projects,
  project,
  missing: projectMissing,
  isPending: projectsPending,
  error: projectsError,
} = useCurrentProject(() => props.workspaceId)
const mobileOpen = ref(false)
watch(
  () => route.fullPath,
  () => (mobileOpen.value = false),
)
</script>

<template>
  <div class="flex min-h-screen bg-base-100">
    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="opacity-0"
      leave-active-class="transition duration-150 ease-in"
      leave-to-class="opacity-0"
    >
      <div
        v-if="mobileOpen"
        class="fixed inset-0 z-30 bg-neutral/40 lg:hidden"
        aria-hidden="true"
        @click="mobileOpen = false"
      ></div>
    </Transition>
    <aside
      class="fixed inset-y-0 left-0 z-40 flex w-64 flex-col border-r border-base-300 bg-base-200 transition-transform duration-200 ease-out lg:sticky lg:top-0 lg:h-screen lg:translate-x-0"
      :class="mobileOpen ? 'translate-x-0' : '-translate-x-full'"
      aria-label="Workspace"
    >
      <div class="flex items-center gap-1 border-b border-base-300 p-2">
        <div class="min-w-0 flex-1">
          <WorkspaceSwitcher :workspace-id="workspaceId" :name="workspace?.name" />
        </div>
        <button
          type="button"
          class="btn btn-square btn-ghost btn-sm lg:hidden"
          aria-label="Close menu"
          @click="mobileOpen = false"
        >
          <X class="size-4" aria-hidden="true" />
        </button>
      </div>
      <nav class="flex-1 overflow-y-auto p-2" aria-label="Project">
        <p class="px-2.5 pt-1 pb-1.5 text-xs font-medium text-base-content/70">Project</p>
        <template v-if="projects.length > 0">
          <ProjectSwitcher :workspace-id="workspaceId" :projects="projects" :current="project" />
          <ul v-if="project" class="mt-1 flex flex-col gap-0.5">
            <li v-for="item in visible(projectNav)" :key="item.path">
              <SidebarLink :item="item" :to="projectPath(workspaceId, project.id, item.path)" />
            </li>
          </ul>
        </template>
        <div
          v-else-if="!projectsPending && !projectsError"
          class="px-2.5 py-2 text-sm text-base-content/80"
        >
          <p>No projects yet.</p>
          <RouterLink :to="`/w/${workspaceId}/projects`" class="link font-medium link-hover">
            Create a project
          </RouterLink>
        </div>
      </nav>
      <nav class="border-t border-base-300 p-2" aria-label="Workspace">
        <ul class="flex flex-col gap-0.5">
          <li v-for="item in visible(workspaceNav)" :key="item.path">
            <SidebarLink :item="item" :to="`/w/${workspaceId}/${item.path}`" />
          </li>
        </ul>
      </nav>
      <div class="border-t border-base-300 p-2"><UserMenu placement="above" /></div>
    </aside>

    <div class="flex min-w-0 flex-1 flex-col">
      <header class="flex h-14 items-center gap-2 border-b border-base-300 px-4 lg:hidden">
        <button
          type="button"
          class="btn btn-square btn-ghost btn-sm"
          aria-label="Open menu"
          @click="mobileOpen = true"
        >
          <MenuIcon class="size-5" aria-hidden="true" />
        </button>
        <span class="truncate font-semibold">{{ workspace?.name }}</span>
      </header>
      <main class="flex-1 px-4 py-6 sm:px-8">
        <div class="mx-auto max-w-5xl">
          <ErrorAlert v-if="error" :error="error" />
          <EmptyState
            v-else-if="projectMissing"
            :icon="FolderX"
            title="Project not found"
            message="It may have been deleted, or the link is wrong."
          >
            <RouterLink :to="`/w/${workspaceId}/projects`" class="btn btn-primary btn-sm">
              All projects
            </RouterLink>
          </EmptyState>
          <RouterView v-else v-slot="{ Component, route: child }">
            <Transition
              mode="out-in"
              enter-active-class="transition duration-150 ease-out"
              enter-from-class="translate-y-1 opacity-0"
              leave-active-class="transition duration-100 ease-in"
              leave-to-class="opacity-0"
            >
              <component
                :is="Component"
                :key="child.path"
                :workspace-id="workspaceId"
                :workspace-name="workspace?.name ?? ''"
                :project-name="project?.name ?? ''"
                :is-admin="isAdmin"
              />
            </Transition>
          </RouterView>
        </div>
      </main>
    </div>
  </div>
</template>
