<script setup lang="ts">
import { GripVertical, Info, X } from 'lucide-vue-next'
import { useId, useSlots } from 'vue'
import type { RouteLocationRaw } from 'vue-router'

import type { HandleBindings } from './DashboardGrid.vue'
import ActionsMenu from './ActionsMenu.vue'
import { TILE_WIDTHS, type TileWidth } from './dashboardGrid'
import Popover from './Popover.vue'
import SegmentedControl from './SegmentedControl.vue'

/**
 * One tile of a `DashboardGrid`: a header with the title (a link to the
 * thing it shows), an info button that opens the `description`, and the
 * `menu` slot's `MenuItem`s behind a "..." button; then the body, which
 * grows to the row's height. While `editing`, a bar on top holds the drag
 * handle, the width control and Remove, the menu hides, and the title
 * stops being a link so a stray click can't leave the page with unsaved
 * changes.
 */
defineProps<{
  title: string
  description?: string
  to: RouteLocationRaw
  editing: boolean
  handle: HandleBindings
  /** Disables the edit controls, for while the layout saves. */
  locked?: boolean
}>()
const width = defineModel<TileWidth>('width', { required: true })
defineEmits<{ remove: [] }>()
const headingId = useId()
const slots = useSlots()
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
        :disabled="locked"
      >
        <GripVertical class="size-4" aria-hidden="true" />
      </button>
      <SegmentedControl
        v-model="width"
        class="ml-auto"
        :options="TILE_WIDTHS"
        :label="`Width of ${title}`"
        size="sm"
        :disabled="locked"
      />
      <button
        type="button"
        class="btn btn-square btn-ghost btn-sm"
        :aria-label="`Remove ${title}`"
        :disabled="locked"
        @click="$emit('remove')"
      >
        <X class="size-4" aria-hidden="true" />
      </button>
    </div>
    <header class="flex min-w-0 items-center gap-1 border-b border-base-300 py-2 pr-2 pl-4">
      <span v-if="editing" :id="headingId" class="truncate font-medium">{{ title }}</span>
      <RouterLink v-else :id="headingId" :to="to" class="link truncate font-medium">{{
        title
      }}</RouterLink>
      <Popover v-if="description" :label="`About ${title}`">
        <template #trigger="{ open, toggle, controls }">
          <button
            type="button"
            class="btn btn-square btn-ghost text-base-content/70 btn-xs"
            :aria-label="`About ${title}`"
            :aria-expanded="open"
            :aria-controls="controls"
            @click="toggle"
          >
            <Info class="size-4" aria-hidden="true" />
          </button>
        </template>
        <p class="text-sm">{{ description }}</p>
      </Popover>
      <div v-if="slots.menu && !editing" class="ml-auto">
        <ActionsMenu :label="`Actions for ${title}`">
          <slot name="menu" />
        </ActionsMenu>
      </div>
    </header>
    <div class="flex flex-1 flex-col p-4"><slot /></div>
  </section>
</template>
