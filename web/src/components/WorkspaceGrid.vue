<script setup lang="ts">
import { Plus, Settings } from 'lucide-vue-next'

import Avatar from '@/components/ui/Avatar.vue'
import type { Membership } from '@/queries/workspaces'

/**
 * The user's workspaces as cards, with a last card that starts a new one.
 * Admins get a shortcut to the workspace's settings on their cards.
 */
defineProps<{ memberships: Membership[]; loading?: boolean }>()
defineEmits<{ create: [] }>()
</script>

<template>
  <div v-if="loading" class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
    <div v-for="n in 3" :key="n" class="h-24 animate-pulse rounded-box bg-base-300"></div>
  </div>
  <ul v-else class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
    <li v-for="m in memberships" :key="m.workspace.id" class="relative">
      <RouterLink
        :to="`/w/${m.workspace.id}`"
        class="flex h-full items-start gap-3 rounded-box border border-base-300 bg-base-100 p-4 transition hover:border-base-content/40 hover:shadow-sm"
      >
        <Avatar :name="m.workspace.name" size="lg" />
        <span class="flex min-w-0 flex-col">
          <span class="truncate font-medium">{{ m.workspace.name }}</span>
          <span class="truncate text-sm text-base-content/70">{{ m.workspace.slug }}</span>
          <span class="mt-2 text-xs capitalize">{{ m.role }}</span>
        </span>
      </RouterLink>
      <RouterLink
        v-if="m.role === 'admin'"
        :to="`/w/${m.workspace.id}/settings`"
        class="btn absolute top-2 right-2 btn-square btn-ghost btn-sm"
        :aria-label="`Settings for ${m.workspace.name}`"
      >
        <Settings class="size-4" aria-hidden="true" />
      </RouterLink>
    </li>
    <li>
      <button
        type="button"
        class="flex h-full min-h-24 w-full items-center justify-center gap-2 rounded-box border border-dashed border-base-300 p-4 text-sm font-medium transition hover:border-base-content/40 hover:bg-base-100"
        @click="$emit('create')"
      >
        <Plus class="size-4" aria-hidden="true" />New workspace
      </button>
    </li>
  </ul>
</template>
