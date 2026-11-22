<script setup lang="ts" generic="T">
import type { Component } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import EmptyState from '@/components/EmptyState.vue'

import SkeletonRows from './SkeletonRows.vue'

export interface Column {
  /** Names the cell slot: `#cell-<key>`. Without a slot the cell is empty. */
  key: string
  label: string
  /** Visually hidden header, for action columns. */
  hidden?: boolean
  /** Classes for the header and cells, such as a width or `text-right`. */
  class?: string
}

/**
 * The app's table: skeleton while loading, the error, an empty state, or the
 * rows. Fill cells with `#cell-<key>="{ row }"` slots.
 */
withDefaults(
  defineProps<{
    columns: Column[]
    rows: T[] | undefined
    rowKey: (row: T) => string
    loading?: boolean
    error?: unknown
    empty?: { title: string; message?: string; icon?: Component }
    rowClass?: (row: T) => string
    label?: string
  }>(),
  { loading: false, error: undefined, empty: undefined, rowClass: undefined, label: undefined },
)

defineSlots<
  {
    [K in `cell-${string}`]?: (props: { row: T }) => unknown
  } & {
    /** Actions under the empty state, such as a create button. */
    'empty-action'?: () => unknown
  }
>()
</script>

<template>
  <SkeletonRows v-if="loading" />
  <ErrorAlert v-else-if="error" :error="error" />
  <EmptyState
    v-else-if="!rows?.length && empty"
    :title="empty.title"
    :message="empty.message"
    :icon="empty.icon"
  >
    <slot name="empty-action" />
  </EmptyState>
  <div v-else class="overflow-x-auto rounded-box border border-base-300">
    <table class="table" :aria-label="label">
      <thead>
        <tr>
          <th v-for="column in columns" :key="column.key" scope="col" :class="column.class">
            <span :class="column.hidden ? 'sr-only' : ''">{{ column.label }}</span>
          </th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="row in rows"
          :key="rowKey(row)"
          class="transition-colors hover:bg-base-200"
          :class="rowClass?.(row)"
        >
          <td v-for="column in columns" :key="column.key" :class="column.class">
            <slot :name="`cell-${column.key}`" :row="row" />
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
