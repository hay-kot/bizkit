<script setup lang="ts">
/**
 * The heading of a page section: a title, an optional line of description,
 * and actions on the right. Use it above a table or inside a `Card`.
 */
defineProps<{ title: string; description?: string; id?: string }>()
</script>

<template>
  <div class="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
    <div class="min-w-0">
      <h2 :id="id" class="text-sm font-semibold">{{ title }}</h2>
      <p
        v-if="description || $slots.description"
        class="mt-0.5 max-w-prose text-xs text-base-content/70"
      >
        <slot name="description">{{ description }}</slot>
      </p>
    </div>
    <div v-if="$slots.actions" class="flex shrink-0 items-center gap-2">
      <slot name="actions" />
    </div>
  </div>
</template>
