<script setup lang="ts">
import type { NavItem } from '@/modules/types'

defineProps<{ item: NavItem; to: string }>()
</script>

<template>
  <RouterLink
    :to="to"
    class="relative flex items-center gap-2.5 rounded-field px-3 py-2 text-sm transition-colors hover:bg-raised"
    active-class="bg-primary/10 font-medium before:absolute before:inset-y-1.5 before:left-0 before:w-0.75 before:rounded-full before:bg-primary hover:bg-primary/15"
  >
    <component :is="item.icon" class="size-4 shrink-0" aria-hidden="true" />
    {{ item.label }}
  </RouterLink>
</template>
