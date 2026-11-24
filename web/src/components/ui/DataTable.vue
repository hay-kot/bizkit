<script setup lang="ts" generic="T">
import { computed, watchEffect, type Component } from 'vue'
import type { RouteLocationRaw } from 'vue-router'

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
  /**
   * The cell that carries the row's link or button when rows are clickable.
   * Defaults to the first column that is neither hidden nor interactive.
   */
  primary?: boolean
  /**
   * The cell holds its own controls (buttons, menus, a copy button), so it
   * sits above the row's hit area and clicks in it never open the row.
   */
  interactive?: boolean
}

/**
 * The app's table: skeleton while loading, the error, an empty state, or the
 * rows. Fill cells with `#cell-<key>="{ row }"` slots.
 *
 * `rowTo` makes each row a link to a page (return `undefined` for a row that
 * goes nowhere) and `@row-click` makes each row open something on the page,
 * such as a panel. Either way the primary cell renders the one real link or
 * button, and its hit area stretches over the row.
 */
const props = withDefaults(
  defineProps<{
    columns: Column[]
    rows: T[] | undefined
    rowKey: (row: T) => string
    rowTo?: (row: T) => RouteLocationRaw | undefined
    /**
     * A tooltip for the row's link or button. The hit area covers the row, so
     * a `title` on anything else in a clickable row never shows.
     */
    rowTitle?: (row: T) => string | undefined
    loading?: boolean
    error?: unknown
    empty?: { title: string; message?: string; icon?: Component }
    rowClass?: (row: T) => string
    label?: string
    /** Declared as a prop, not an emit, so the table knows whether rows are clickable. */
    onRowClick?: (row: T) => void
  }>(),
  {
    rowTo: undefined,
    rowTitle: undefined,
    loading: false,
    error: undefined,
    empty: undefined,
    rowClass: undefined,
    label: undefined,
    onRowClick: undefined,
  },
)

defineSlots<
  {
    [K in `cell-${string}`]?: (props: { row: T }) => unknown
  } & {
    /** Actions under the empty state, such as a create button. */
    'empty-action'?: () => unknown
  }
>()

const primaryKey = computed(
  () =>
    (props.columns.find((c) => c.primary) ?? props.columns.find((c) => !c.hidden && !c.interactive))
      ?.key,
)

if (import.meta.env.DEV) {
  watchEffect(() => {
    if (!props.rowTo && !props.onRowClick) return
    const at = props.columns.findIndex((c) => c.key === primaryKey.value)
    if (props.columns.slice(0, at + 1).some((c) => c.interactive)) {
      console.warn(
        `DataTable: the primary column "${primaryKey.value}" is interactive or comes after an interactive column, so the row's hit area covers that column's controls.`,
      )
    }
  })
}

const entries = computed(() => (props.rows ?? []).map((row) => ({ row, to: props.rowTo?.(row) })))

// One real link or button per row, stretched over the row by its ::after,
// rather than a <tr role="link"> with a click handler: a real link keeps
// middle-click, Cmd-click, the status-bar URL and one tab stop per row, and
// screen readers announce it by its name.
const target = [
  'font-medium text-primary after:absolute after:inset-0',
  'focus-visible:outline-none focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-primary',
]

function clickable(to: RouteLocationRaw | undefined): boolean {
  return to !== undefined || props.onRowClick !== undefined
}

function cellClass(column: Column) {
  // Only `relative`, no z-index: a z-index would make each cell a stacking
  // context, and a menu opened in one row would slide under the next row's
  // cell. Positioned cells after the primary one already paint above its
  // ::after, so keep the primary column before interactive ones.
  return [column.class, column.interactive ? 'relative' : '']
}
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
  <div v-else class="overflow-x-auto rounded-box border border-base-300 bg-base-100">
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
          v-for="{ row, to } in entries"
          :key="rowKey(row)"
          :class="[
            clickable(to) ? 'relative cursor-pointer transition-colors hover:bg-base-200' : '',
            rowClass?.(row),
          ]"
        >
          <td v-for="column in columns" :key="column.key" :class="cellClass(column)">
            <RouterLink
              v-if="column.key === primaryKey && to !== undefined"
              :to="to"
              :title="rowTitle?.(row)"
              :class="target"
            >
              <slot :name="`cell-${column.key}`" :row="row" />
            </RouterLink>
            <button
              v-else-if="column.key === primaryKey && onRowClick"
              type="button"
              class="cursor-pointer text-start"
              :class="target"
              :title="rowTitle?.(row)"
              @click="onRowClick(row)"
            >
              <slot :name="`cell-${column.key}`" :row="row" />
            </button>
            <slot v-else :name="`cell-${column.key}`" :row="row" />
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
