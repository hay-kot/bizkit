<script setup lang="ts">
import type { RouteLocationRaw } from 'vue-router'

import { stretchedTarget } from './stretchedTarget'

/**
 * A card in a `CardGrid` that opens one thing. The title is the card's one
 * real link and its hit area covers the card; `actions` sit in the top right
 * corner above it.
 */
defineProps<{ to: RouteLocationRaw; title: string; description?: string }>()

defineSlots<{
  leading?: () => unknown
  meta?: () => unknown
  actions?: () => unknown
}>()
</script>

<template>
  <div
    class="relative flex h-full items-start gap-3 rounded-box border border-base-300 bg-base-100 p-4 transition-colors hover:border-base-content/40"
  >
    <slot name="leading" />
    <div class="flex min-w-0 flex-1 flex-col">
      <RouterLink :to="to" class="truncate after:rounded-box" :class="stretchedTarget">{{
        title
      }}</RouterLink>
      <span v-if="description" class="truncate text-sm text-base-content/70">{{
        description
      }}</span>
      <div v-if="$slots.meta" class="mt-2 text-xs text-base-content/70"><slot name="meta" /></div>
    </div>
    <div v-if="$slots.actions" class="relative flex shrink-0 gap-1"><slot name="actions" /></div>
  </div>
</template>
