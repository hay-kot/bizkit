<script setup lang="ts">
import { onKeyStroke } from '@vueuse/core'
import { FolderX, Menu as MenuIcon, Search, X } from 'lucide-vue-next'
import { inject, ref, unref, watch } from 'vue'
import { useRoute, viewDepthKey } from 'vue-router'

import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import PaletteRows from '@/components/PaletteRows.vue'
import ProjectSwitcher from '@/components/ProjectSwitcher.vue'
import SidebarLink from '@/components/SidebarLink.vue'
import UserMenu from '@/components/UserMenu.vue'
import WorkspaceSwitcher from '@/components/WorkspaceSwitcher.vue'
import CommandPalette from '@/components/ui/CommandPalette.vue'
import { useCurrentProject } from '@/composables/useCurrentProject'
import { projectNav, projectPath, workspaceNav } from '@/modules'
import type { NavItem } from '@/modules/types'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { routeKey } from '@/lib/routeKey'
import { closePalette, openPalette, paletteOpen } from '@/palette/state'
import { useMe } from '@/queries/auth'
import { useWorkspace } from '@/queries/workspaces'

const props = defineProps<{ workspaceId: string }>()
const route = useRoute()
// The depth of this layout's own RouterView, for keying its pages.
const depth = unref(inject(viewDepthKey, 0))
const { data: workspace, error } = useWorkspace(() => props.workspaceId)
const isAdmin = useIsAdmin(() => props.workspaceId)
// The palette's sources mount once the role is known: a remount when the
// admin flag flips would cancel their list requests mid-flight, and the
// palette would miss those rows.
const { data: me } = useMe()
const visible = (items: NavItem[]) => items.filter((item) => !item.adminOnly || isAdmin.value)
const {
  projects,
  project,
  missing: projectMissing,
  isPending: projectsPending,
  error: projectsError,
} = useCurrentProject(() => props.workspaceId)
const mobileOpen = ref(false)
const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform)

onKeyStroke(
  // Chrome autofill sends keydown events without a `key`.
  (event) =>
    typeof event.key === 'string' &&
    event.key.toLowerCase() === 'k' &&
    (event.metaKey || event.ctrlKey),
  (event) => {
    event.preventDefault()
    if (paletteOpen.value) closePalette()
    else openPalette()
  },
)
watch(
  () => route.fullPath,
  () => (mobileOpen.value = false),
)
</script>

<template>
  <div class="flex min-h-screen bg-base-200">
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
    <!-- The column paints the sidebar's background down the whole page; the
    sticky aside inside it stays in view while the content scrolls. -->
    <div
      class="contents lg:block lg:w-64 lg:shrink-0 lg:border-r lg:border-base-300 lg:bg-base-200"
    >
      <aside
        class="fixed inset-y-0 left-0 z-40 flex w-64 flex-col border-r border-base-300 bg-base-200 transition-transform duration-200 ease-out lg:sticky lg:top-0 lg:h-screen lg:translate-x-0 lg:border-r-0"
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
        <!-- The project is a control, not a page: it gets a field-like box of
        its own, apart from the nav rows, so it isn't clicked by mistake. -->
        <div v-if="projects.length > 0" class="px-2 pt-3">
          <ProjectSwitcher :workspace-id="workspaceId" :projects="projects" :current="project" />
        </div>
        <div class="px-2 pt-2">
          <button
            type="button"
            class="flex h-9 w-full items-center gap-2.5 rounded-field border border-base-300 bg-base-100 px-3 text-sm text-base-content/70 transition-colors hover:border-base-content/60 hover:bg-raised"
            aria-keyshortcuts="Meta+K Control+K"
            data-testid="open-palette"
            @click="openPalette()"
          >
            <Search class="size-4 shrink-0" aria-hidden="true" />
            <span class="flex-1 text-left">Search…</span>
            <kbd class="font-sans text-xs">{{ isMac ? '⌘K' : 'Ctrl K' }}</kbd>
          </button>
        </div>
        <nav
          class="mt-3 flex-1 overflow-y-auto border-t border-base-300 p-2 pt-3"
          aria-label="Project"
        >
          <ul v-if="projects.length > 0 && project" class="flex flex-col gap-0.5">
            <li v-for="item in visible(projectNav)" :key="item.path">
              <SidebarLink :item="item" :to="projectPath(workspaceId, project.id, item.path)" />
            </li>
          </ul>
          <div
            v-else-if="projects.length === 0 && !projectsPending && !projectsError"
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
    </div>

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
      <main class="min-w-0 flex-1 px-4 py-6 sm:px-6 lg:px-8">
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
              :key="routeKey(child, depth)"
              :workspace-id="workspaceId"
              :workspace-name="workspace?.name ?? ''"
              :project-name="project?.name ?? ''"
              :is-admin="isAdmin"
            />
          </Transition>
        </RouterView>
      </main>
    </div>
    <PaletteRows
      v-if="me"
      :key="`${workspaceId}:${project?.id}:${isAdmin}`"
      :workspace-id="workspaceId"
      :project-id="project?.id"
      :project-name="project?.name ?? ''"
      :workspace-name="workspace?.name ?? 'Workspace'"
      :is-admin="isAdmin"
    />
    <CommandPalette />
  </div>
</template>
