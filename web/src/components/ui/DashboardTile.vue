<script setup lang="ts">
import { GripVertical, X } from 'lucide-vue-next'
import { useId } from 'vue'
import type { RouteLocationRaw } from 'vue-router'

import type { HandleBindings } from './DashboardGrid.vue'
import { TILE_WIDTHS, type TileWidth } from './dashboardGrid'
import SegmentedControl from './SegmentedControl.vue'

/**
 * One tile of a `DashboardGrid`: the title (a link to the thing it shows),
 * a muted caption, then the body, which grows to the row's height. While
 * `editing`, a bar on top holds the drag handle, the width control and
 * Remove, and the title stops being a link so a stray click can't leave
 * the page with unsaved changes.
 */
defineProps<{
  title: string
  description?: string
  to: RouteLocationRaw
  editing: boolean
  handle: HandleBindings
}>()
const width = defineModel<TileWidth>('width', { required: true })
defineEmits<{ remove: [] }>()
const headingId = useId()
</script>

<template>
  <section
    class="flex h-full flex-col rounded-box border border-base-300 bg-base-100"
    :aria-labelledby="headingId"
  >
    <div v-if="editing" class="flex items-center gap-2 border-b border-base-300 px-2 py-1.5">
      <button
        type="button"
        class="btn btn-square cursor-grab touch-none btn-ghost btn-sm active:cursor-grabbing"
        v-bind="handle"
      >
        <GripVertical class="size-4" aria-hidden="true" />
      </button>
      <SegmentedControl
        v-model="width"
        class="ml-auto"
        :options="TILE_WIDTHS"
        :label="`Width of ${title}`"
        size="sm"
      />
      <button
        type="button"
        class="btn btn-square btn-ghost btn-sm"
        :aria-label="`Remove ${title}`"
        @click="$emit('remove')"
      >
        <X class="size-4" aria-hidden="true" />
      </button>
    </div>
    <header class="flex min-w-0 flex-col px-4 pt-4">
      <span v-if="editing" :id="headingId" class="truncate font-medium">{{ title }}</span>
      <RouterLink v-else :id="headingId" :to="to" class="link truncate font-medium">{{
        title
      }}</RouterLink>
      <span v-if="description" class="truncate text-xs text-base-content/70">{{
        description
      }}</span>
    </header>
    <div class="flex flex-1 flex-col px-4 pt-4 pb-4"><slot /></div>
  </section>
</template>
