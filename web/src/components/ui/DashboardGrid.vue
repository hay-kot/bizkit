<script setup lang="ts">
import { useMediaQuery } from '@vueuse/core'
import { computed, nextTick, ref, useId } from 'vue'

import { type DropTarget, dropIndex, type TileWidth } from './dashboardGrid'

export interface GridItem {
  id: string
  width: TileWidth
  /** What the move announcement calls the tile. */
  label: string
}

/** Bind on the tile's drag handle with `v-bind`. */
export interface HandleBindings {
  'data-grid-handle': string
  'aria-label': string
  'aria-describedby': string
  onPointerdown: (event: PointerEvent) => void
  onPointermove: (event: PointerEvent) => void
  onPointerup: (event: PointerEvent) => void
  onPointercancel: () => void
  onKeydown: (event: KeyboardEvent) => void
}

/**
 * Dashboard tiles on a 12-column grid, one column below `lg`. A row is as
 * tall as its tallest tile and each cell stretches to it, so no height is
 * stored. While `editing`, each tile's handle reorders it by pointer drag
 * or by the arrow keys, and every move is announced. `locked` stops every
 * move, for while the layout saves.
 */
const props = withDefaults(
  defineProps<{ items: GridItem[]; editing: boolean; locked?: boolean }>(),
  {
    locked: false,
  },
)
const emit = defineEmits<{ move: [from: number, to: number] }>()
defineSlots<{
  tile: (props: { item: GridItem; index: number; handle: HandleBindings }) => unknown
}>()

// Static strings, so Tailwind sees every class.
const SPAN: Record<TileWidth, string> = {
  4: 'lg:col-span-4',
  6: 'lg:col-span-6',
  8: 'lg:col-span-8',
  12: 'lg:col-span-12',
}
// The bar sits in the middle of the gap-4 gutter beside the target tile:
// sideways on the 12-column grid, above or below in the one column.
const INDICATOR = {
  wide: {
    before:
      'before:absolute before:inset-y-0 before:-left-2.5 before:w-1 before:rounded-full before:bg-primary',
    after:
      'after:absolute after:inset-y-0 after:-right-2.5 after:w-1 after:rounded-full after:bg-primary',
  },
  narrow: {
    before:
      'before:absolute before:inset-x-0 before:-top-2.5 before:h-1 before:rounded-full before:bg-primary',
    after:
      'after:absolute after:inset-x-0 after:-bottom-2.5 after:h-1 after:rounded-full after:bg-primary',
  },
}

const wide = useMediaQuery('(min-width: 64rem)')
const hintId = useId()
const root = ref<HTMLElement>()
const cells = ref<HTMLElement[]>([])
const dragFrom = ref<number>()
const target = ref<DropTarget>()
const announcement = ref('')

const landing = computed(() =>
  dragFrom.value === undefined || !target.value
    ? undefined
    : dropIndex(dragFrom.value, target.value),
)

function cellClass(index: number): string[] {
  const classes = [SPAN[props.items[index]?.width ?? 6]]
  if (dragFrom.value === index) classes.push('opacity-50')
  if (landing.value !== undefined && target.value?.index === index) {
    const bars = wide.value ? INDICATOR.wide : INDICATOR.narrow
    classes.push(target.value.after ? bars.after : bars.before)
  }
  return classes
}

function announce(from: number, to: number) {
  const item = props.items[from]
  if (item)
    announcement.value = `${item.label} moved to position ${to + 1} of ${props.items.length}`
}

function move(from: number, to: number) {
  if (props.locked || to < 0 || to >= props.items.length || to === from) return
  const id = props.items[from]?.id
  announce(from, to)
  emit('move', from, to)
  // Moving a keyed cell can drop focus from its handle in some browsers.
  void nextTick(() => {
    root.value?.querySelector<HTMLElement>(`[data-grid-handle="${id}"]`)?.focus()
  })
}

function insideGrid(x: number, y: number): boolean {
  const box = root.value?.getBoundingClientRect()
  return !!box && x >= box.left && x <= box.right && y >= box.top && y <= box.bottom
}

function targetAt(x: number, y: number): DropTarget | undefined {
  // Template ref arrays don't keep v-for order, so each cell carries its index.
  for (const cell of cells.value) {
    const index = Number(cell.dataset.index)
    const box = cell.getBoundingClientRect()
    if (x < box.left || x > box.right || y < box.top || y > box.bottom) continue
    const after = wide.value ? x > box.left + box.width / 2 : y > box.top + box.height / 2
    return { index, after }
  }
  return undefined
}

function endDrag() {
  dragFrom.value = undefined
  target.value = undefined
}

function handle(item: GridItem, index: number): HandleBindings {
  return {
    'data-grid-handle': item.id,
    'aria-label': `Move ${item.label}`,
    'aria-describedby': hintId,
    onPointerdown(event) {
      if (event.button !== 0 || props.locked) return
      event.preventDefault()
      ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
      dragFrom.value = index
      target.value = undefined
    },
    onPointermove(event) {
      if (dragFrom.value === undefined) return
      // In a gutter the last target holds; outside the grid a drop cancels.
      target.value = insideGrid(event.clientX, event.clientY)
        ? (targetAt(event.clientX, event.clientY) ?? target.value)
        : undefined
    },
    onPointerup() {
      const from = dragFrom.value
      const to = landing.value
      endDrag()
      if (from !== undefined && to !== undefined) move(from, to)
    },
    onPointercancel: endDrag,
    onKeydown(event) {
      if (dragFrom.value !== undefined) {
        if (event.key === 'Escape') endDrag()
        return
      }
      if (props.locked) return
      const last = props.items.length - 1
      const to: Record<string, number> = {
        ArrowLeft: index - 1,
        ArrowUp: index - 1,
        ArrowRight: index + 1,
        ArrowDown: index + 1,
        Home: 0,
        End: last,
      }
      const next = to[event.key]
      if (next === undefined) return
      event.preventDefault()
      move(index, next)
    },
  }
}
</script>

<template>
  <div ref="root" class="grid grid-cols-1 gap-4 lg:grid-cols-12">
    <div
      v-for="(item, index) in items"
      :key="item.id"
      ref="cells"
      :data-index="index"
      class="relative min-w-0"
      :class="cellClass(index)"
    >
      <slot name="tile" :item="item" :index="index" :handle="handle(item, index)" />
    </div>
    <p :id="hintId" class="sr-only">Drag, or press the arrow keys, to move the tile.</p>
    <p class="sr-only" role="status" aria-live="polite">{{ announcement }}</p>
  </div>
</template>
