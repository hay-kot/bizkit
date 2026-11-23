<script setup lang="ts">
/**
 * The top of a page: an optional eyebrow (`section`), then the title with
 * the page's actions on the same row, then the description. Actions go in
 * the default slot.
 */
defineProps<{ title: string; description?: string; section?: string }>()
</script>

<template>
  <header class="flex flex-col gap-1">
    <p v-if="section" class="text-xs font-medium tracking-wide text-base-content/70 uppercase">
      {{ section }}
    </p>
    <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
      <h1 class="min-w-0 text-2xl font-semibold tracking-tight break-words">{{ title }}</h1>
      <div v-if="$slots.default" class="flex flex-wrap items-center gap-2"><slot /></div>
    </div>
    <p v-if="description" class="text-sm text-base-content/80">{{ description }}</p>
  </header>
</template>
