<script setup lang="ts" generic="T">
import type { Component } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import EmptyState from '@/components/EmptyState.vue'

import SkeletonRows from './SkeletonRows.vue'

/**
 * A collection that reads top to bottom, one `ListItem` per entry: skeleton
 * while loading, the error, an empty state, or the items in one bordered
 * panel. Use `DataTable` instead when people compare entries by column.
 */
withDefaults(
  defineProps<{
    items: T[] | undefined
    itemKey: (item: T) => string
    loading?: boolean
    error?: unknown
    empty?: { title: string; message?: string; icon?: Component }
    label?: string
    /** Renders an `ol`, for entries whose order means something (newest first). */
    ordered?: boolean
  }>(),
  { loading: false, error: undefined, empty: undefined, label: undefined, ordered: false },
)

defineSlots<{
  item: (props: { item: T }) => unknown
  /** Actions under the empty state, such as a create button. */
  'empty-action'?: () => unknown
}>()
</script>

<template>
  <SkeletonRows v-if="loading" />
  <ErrorAlert v-else-if="error" :error="error" />
  <EmptyState
    v-else-if="!items?.length && empty"
    :title="empty.title"
    :message="empty.message"
    :icon="empty.icon"
  >
    <slot name="empty-action" />
  </EmptyState>
  <component
    :is="ordered ? 'ol' : 'ul'"
    v-else-if="items?.length"
    class="divide-y divide-base-300 rounded-box border border-base-300 bg-base-100"
    :aria-label="label"
  >
    <li v-for="item in items" :key="itemKey(item)" class="group/list-item">
      <slot name="item" :item="item" />
    </li>
  </component>
</template>
