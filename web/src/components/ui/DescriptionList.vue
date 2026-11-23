<script setup lang="ts">
/**
 * Facts about the thing a page shows, as label and value pairs in a row
 * that wraps. Give an item a `#<key>` slot to render its value yourself.
 */
defineProps<{ items: { key: string; label: string; value?: string }[] }>()
</script>

<template>
  <dl
    class="grid grid-cols-2 gap-x-6 gap-y-3 rounded-box border border-base-300 bg-base-100 px-4 py-3 sm:grid-cols-3 lg:grid-cols-5"
  >
    <div v-for="item in items" :key="item.key" class="min-w-0">
      <dt class="text-xs text-base-content/70">{{ item.label }}</dt>
      <dd class="mt-0.5 truncate text-sm">
        <slot :name="item.key">{{ item.value }}</slot>
      </dd>
    </div>
  </dl>
</template>
