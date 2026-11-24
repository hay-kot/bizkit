<script setup lang="ts">
import {
  onClickOutside,
  onKeyStroke,
  useElementBounding,
  useElementSize,
  useWindowSize,
} from '@vueuse/core'
import { Calendar, ChevronDown, ChevronLeft, ChevronRight, ZoomOut } from 'lucide-vue-next'
import { computed, nextTick, ref, useId, watch } from 'vue'

import { formatDayRange } from '@/lib/format'
import {
  canZoomOut,
  type DateRange,
  MAX_RANGE_DAYS,
  PRESET_GROUPS,
  type PresetOption,
  presetRange,
  rangeLength,
  shiftRange,
  zoomOut,
} from '@/lib/range'

/**
 * A report's range in one toolbar group: shift back, the range (opens the
 * picker), shift forward, zoom out, then the default slot for page controls
 * such as compare. Days are UTC and `today` comes from the caller, like the
 * range. Every change comes out as one `apply`; the page owns the state.
 */
const props = withDefaults(
  defineProps<{
    range: DateRange
    today: string
    presets?: PresetOption[][]
    zoom?: boolean
  }>(),
  { presets: () => PRESET_GROUPS, zoom: true },
)
const emit = defineEmits<{ apply: [range: DateRange] }>()
const slots = defineSlots<{ default?: () => unknown }>()

const open = ref(false)
const root = ref<HTMLElement>()
const trigger = ref<HTMLButtonElement>()
const panel = ref<HTMLElement>()
const panelId = useId()
const from = ref('')
const to = ref('')
// The error waits for an edit, so opening the panel never greets the user
// with one.
const touched = ref(false)
const measured = ref(false)

// Previous, Next, Zoom out and Back change the range while the panel may be
// open; Apply must start from what the page shows, not from when it opened.
watch(
  () => [props.range.from, props.range.to] as const,
  ([nextFrom, nextTo]) => {
    from.value = nextFrom
    to.value = nextTo
    touched.value = false
  },
)

const label = computed(() => {
  const preset = props.presets.flat().find((p) => p.value === props.range.preset)
  return preset?.label ?? formatDayRange(props.range.from, props.range.to)
})
const atToday = computed(() => props.range.to >= props.today)
const absoluteError = computed(() => {
  if (!from.value || !to.value) return 'Pick both days.'
  if (from.value > to.value) return 'From must be on or before To.'
  if (to.value > props.today) return 'To can be today at the latest.'
  if (rangeLength({ from: from.value, to: to.value }) > MAX_RANGE_DAYS)
    return `Pick ${MAX_RANGE_DAYS} days at most.`
  return ''
})

function apply(range: DateRange) {
  emit('apply', range)
}

function pickPreset(preset: PresetOption) {
  apply({ preset: preset.value, ...presetRange(preset.value, props.today) })
  close()
}

function applyAbsolute() {
  if (absoluteError.value) return
  apply({ preset: 'custom', from: from.value, to: to.value })
  close()
}

// The panel renders in <body> so scroll containers can't clip it, below the
// whole group, or above it when there is less room below than FLOOR (or the
// panel's height) and more above. It shifts to stay inside the viewport and
// follows the group when the page scrolls. Below the sm breakpoint the panes
// stack and the panel takes the viewport's width.
const EDGE = 8
const GAP = 4
const SM = 640
const FLOOR = 360
const anchor = useElementBounding(root)
const viewport = useWindowSize({ includeScrollbar: false })
const panelSize = useElementSize(panel, { width: 0, height: 0 }, { box: 'border-box' })
const position = computed(() => {
  const maxWidth = Math.max(0, viewport.width.value - 2 * EDGE)
  const narrow = viewport.width.value < SM
  const left = narrow
    ? EDGE
    : Math.max(
        EDGE,
        Math.min(anchor.left.value, viewport.width.value - EDGE - panelSize.width.value),
      )
  const below = viewport.height.value - anchor.bottom.value - GAP - EDGE
  const above = anchor.top.value - GAP - EDGE
  const up = below < Math.min(FLOOR, panelSize.height.value) && above > below
  return {
    left: `${left}px`,
    maxWidth: `${maxWidth}px`,
    maxHeight: `${Math.max(160, up ? above : below)}px`,
    ...(up
      ? { bottom: `${viewport.height.value - anchor.top.value + GAP}px` }
      : { top: `${anchor.bottom.value + GAP}px` }),
    ...(narrow ? { width: `${maxWidth}px` } : {}),
  }
})

async function show() {
  anchor.update()
  from.value = props.range.from
  to.value = props.range.to
  touched.value = false
  measured.value = false
  open.value = true
  // The panel renders hidden first so its size is known before it is placed;
  // the ResizeObserver behind panelSize reports only after a frame.
  await nextTick()
  if (panel.value) {
    panelSize.width.value = panel.value.offsetWidth
    panelSize.height.value = panel.value.offsetHeight
  }
  measured.value = true
  await nextTick()
  const start =
    panel.value?.querySelector<HTMLElement>('[aria-current="true"]') ??
    panel.value?.querySelector<HTMLElement>('input')
  start?.focus()
}

function close() {
  if (!open.value) return
  open.value = false
  trigger.value?.focus()
}

onClickOutside(root, () => (open.value = false), { ignore: [panel] })
onKeyStroke('Escape', () => close())

function onFocusOut(event: FocusEvent) {
  const next = event.relatedTarget as Node | null
  if (next && !panel.value?.contains(next) && !root.value?.contains(next)) open.value = false
}

const iconButton =
  'flex size-8 shrink-0 items-center justify-center rounded-selector text-base-content/70 transition-colors hover:bg-base-content/10 hover:text-base-content focus-visible:outline-2 focus-visible:outline-primary disabled:pointer-events-none disabled:opacity-40'
</script>

<template>
  <div
    ref="root"
    class="inline-flex h-9 max-w-full min-w-0 items-center gap-0.5 rounded-field border border-field-border bg-field p-0.5"
  >
    <button
      type="button"
      :class="iconButton"
      aria-label="Previous period"
      title="Previous period"
      @click="apply(shiftRange(range, -1, today))"
    >
      <ChevronLeft class="size-4" aria-hidden="true" />
    </button>
    <button
      ref="trigger"
      type="button"
      class="flex h-8 min-w-0 items-center gap-2 rounded-selector px-2.5 text-sm transition-colors hover:bg-base-content/10 focus-visible:outline-2 focus-visible:outline-primary"
      aria-haspopup="dialog"
      :aria-expanded="open"
      :aria-controls="panelId"
      @click="open ? close() : show()"
    >
      <Calendar class="size-4 shrink-0 text-base-content/70" aria-hidden="true" />
      <span class="truncate"><span class="sr-only">Time range: </span>{{ label }}</span>
      <ChevronDown class="size-3.5 shrink-0 text-base-content/70" aria-hidden="true" />
    </button>
    <button
      type="button"
      :class="iconButton"
      aria-label="Next period"
      title="Next period"
      :disabled="atToday"
      @click="apply(shiftRange(range, 1, today))"
    >
      <ChevronRight class="size-4" aria-hidden="true" />
    </button>
    <button
      v-if="zoom"
      type="button"
      :class="iconButton"
      aria-label="Zoom out"
      title="Zoom out"
      :disabled="!canZoomOut(range)"
      @click="apply(zoomOut(range, today))"
    >
      <ZoomOut class="size-4" aria-hidden="true" />
    </button>
    <template v-if="slots.default">
      <span class="mx-0.5 h-5 w-px shrink-0 bg-base-300" aria-hidden="true" />
      <slot />
    </template>
    <Teleport to="body">
      <Transition
        enter-active-class="transition duration-100 ease-out"
        enter-from-class="-translate-y-1 opacity-0"
        leave-active-class="transition duration-75 ease-in"
        leave-to-class="opacity-0"
      >
        <!-- eslint-disable vue/no-restricted-v-bind -- a floating panel needs coordinates from its trigger -->
        <div
          v-if="open"
          :id="panelId"
          ref="panel"
          role="dialog"
          aria-label="Time range"
          :style="position"
          class="fixed z-60 flex flex-col overflow-auto rounded-box border border-base-300 bg-raised shadow-lg sm:flex-row"
          :class="measured ? '' : 'invisible'"
          @focusout="onFocusOut"
        >
          <form
            class="flex flex-col gap-3 border-b border-base-300 p-4 sm:w-64 sm:border-r sm:border-b-0"
            novalidate
            @submit.prevent="applyAbsolute"
          >
            <h2 class="text-xs font-medium text-base-content/70">Absolute range</h2>
            <label class="flex flex-col gap-1 text-sm">
              From
              <input
                v-model="from"
                type="date"
                class="input h-9 w-full"
                :max="today"
                @input="touched = true"
              />
            </label>
            <label class="flex flex-col gap-1 text-sm">
              To
              <input
                v-model="to"
                type="date"
                class="input h-9 w-full"
                :max="today"
                @input="touched = true"
              />
            </label>
            <p v-if="absoluteError && touched" class="text-xs text-error" role="alert">
              {{ absoluteError }}
            </p>
            <button
              type="submit"
              class="btn h-9 w-full btn-primary btn-sm"
              :disabled="!!absoluteError"
            >
              Apply
            </button>
          </form>
          <div class="flex flex-col p-1.5 sm:w-52">
            <h2 class="px-3 pt-2 pb-1 text-xs font-medium text-base-content/70">Quick ranges</h2>
            <template v-for="(group, index) in presets" :key="index">
              <hr v-if="index > 0" class="mx-3 my-1 border-base-300" />
              <button
                v-for="preset in group"
                :key="preset.value"
                type="button"
                class="flex min-h-9 items-center gap-2.5 rounded-field px-3 py-2 text-left text-sm transition-colors hover:bg-base-content/5 focus-visible:bg-base-content/5 focus-visible:outline-none"
                :class="preset.value === range.preset ? 'font-medium text-primary' : ''"
                :aria-current="preset.value === range.preset ? 'true' : undefined"
                @click="pickPreset(preset)"
              >
                <span
                  class="h-4 w-0.5 shrink-0 rounded-full"
                  :class="preset.value === range.preset ? 'bg-primary' : 'bg-transparent'"
                  aria-hidden="true"
                />
                {{ preset.label }}
              </button>
            </template>
          </div>
        </div>
        <!-- eslint-enable vue/no-restricted-v-bind -->
      </Transition>
    </Teleport>
  </div>
</template>
