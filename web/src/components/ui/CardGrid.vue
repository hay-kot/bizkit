<script setup lang="ts" generic="T">
/**
 * A small set of big things (workspaces, dashboards) as a responsive grid of
 * `LinkCard`s, so the page that shows them stays one column. The `end` slot
 * is the grid's last cell, for a dashed "New ..." card.
 */
withDefaults(
  defineProps<{
    items: T[] | undefined
    itemKey: (item: T) => string
    loading?: boolean
  }>(),
  { loading: false },
)

defineSlots<{
  item: (props: { item: T }) => unknown
  end?: () => unknown
}>()

const grid = 'grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4'
</script>

<template>
  <div v-if="loading" :class="grid" aria-busy="true" aria-label="Loading">
    <div v-for="n in 3" :key="n" class="h-24 animate-pulse rounded-box bg-base-300"></div>
  </div>
  <ul v-else :class="grid">
    <li v-for="item in items" :key="itemKey(item)">
      <slot name="item" :item="item" />
    </li>
    <li v-if="$slots.end"><slot name="end" /></li>
  </ul>
</template>
