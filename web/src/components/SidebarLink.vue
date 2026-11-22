<script setup lang="ts">
import type { NavItem } from '@/modules/types'

defineProps<{ item: NavItem; to: string }>()
</script>

<template>
  <RouterLink
    :to="to"
    class="flex items-center gap-2.5 rounded-field px-2.5 py-2 text-sm transition-colors hover:bg-base-300"
    active-class="bg-base-100 font-medium shadow-sm hover:bg-base-100"
  >
    <component :is="item.icon" class="size-4 shrink-0" aria-hidden="true" />
    {{ item.label }}
  </RouterLink>
</template>
