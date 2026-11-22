<script setup lang="ts">
import { FolderKanban, KeyRound, Mail, Menu as MenuIcon, Settings, Users, X } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import UserMenu from '@/components/UserMenu.vue'
import WorkspaceSwitcher from '@/components/WorkspaceSwitcher.vue'
import { useMe } from '@/queries/auth'
import { useWorkspace } from '@/queries/workspaces'

const props = defineProps<{ workspaceId: string }>()
const route = useRoute()
const { data: workspace, error } = useWorkspace(() => props.workspaceId)
const { data: me } = useMe()
const isAdmin = computed(
  () => me.value?.memberships.find((m) => m.workspace.id === props.workspaceId)?.role === 'admin',
)
const nav = computed(() => [
  { to: 'projects', label: 'Projects', icon: FolderKanban, show: true },
  { to: 'members', label: 'Members', icon: Users, show: true },
  { to: 'invitations', label: 'Invitations', icon: Mail, show: isAdmin.value },
  { to: 'api-keys', label: 'API keys', icon: KeyRound, show: isAdmin.value },
  { to: 'settings', label: 'Settings', icon: Settings, show: isAdmin.value },
])
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
      <nav class="flex-1 overflow-y-auto p-2" aria-label="Workspace sections">
        <ul class="flex flex-col gap-0.5">
          <template v-for="item in nav" :key="item.to">
            <li v-if="item.show">
              <RouterLink
                :to="`/w/${workspaceId}/${item.to}`"
                class="flex items-center gap-2.5 rounded-field px-2.5 py-2 text-sm transition-colors hover:bg-base-300"
                active-class="bg-base-100 font-medium shadow-sm hover:bg-base-100"
              >
                <component :is="item.icon" class="size-4 shrink-0" aria-hidden="true" />
                {{ item.label }}
              </RouterLink>
            </li>
          </template>
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
                :is-admin="isAdmin"
              />
            </Transition>
          </RouterView>
        </div>
      </main>
    </div>
  </div>
</template>
