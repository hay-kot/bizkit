<script setup lang="ts">
import { ChevronsUpDown, LayoutGrid, LogOut } from 'lucide-vue-next'
import { useRouter } from 'vue-router'

import Avatar from '@/components/ui/Avatar.vue'
import Menu from '@/components/ui/Menu.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import { useMe, useSignOut } from '@/queries/auth'

withDefaults(defineProps<{ placement?: 'below' | 'above'; compact?: boolean }>(), {
  placement: 'below',
  compact: false,
})
const router = useRouter()
const { data: me } = useMe()
const { mutateAsync: signOut } = useSignOut()

async function onSignOut() {
  await signOut({})
  await router.replace('/sign-in')
}
</script>

<template>
  <Menu label="Account" :placement="placement" :align="compact ? 'end' : 'start'">
    <template #trigger="{ open, toggle, controls }">
      <button
        type="button"
        class="flex w-full items-center gap-2 rounded-field px-2 py-1.5 text-left text-sm transition-colors hover:bg-base-300"
        :aria-expanded="open"
        :aria-controls="controls"
        aria-haspopup="menu"
        data-testid="user-menu"
        @click="toggle"
      >
        <Avatar :name="me?.email ?? '?'" size="sm" />
        <span v-if="!compact" class="flex-1 truncate" data-testid="signed-in-as">{{
          me?.email
        }}</span>
        <ChevronsUpDown
          v-if="!compact"
          class="size-4 shrink-0 text-base-content/70"
          aria-hidden="true"
        />
      </button>
    </template>
    <div class="px-2 py-1.5 text-xs text-base-content/70">{{ me?.email }}</div>
    <MenuItem to="/"><LayoutGrid class="size-4" aria-hidden="true" />Your workspaces</MenuItem>
    <MenuItem @select="onSignOut"><LogOut class="size-4" aria-hidden="true" />Sign out</MenuItem>
  </Menu>
</template>
