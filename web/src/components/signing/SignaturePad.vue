<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'

import Field from '@/components/ui/Field.vue'
import SegmentedControl from '@/components/ui/SegmentedControl.vue'
import {
  MAX_POINTS,
  MAX_STROKES,
  MAX_TYPED,
  PAD_HEIGHT,
  PAD_WIDTH,
  type SigningSignature,
} from '@/queries/signing'

/**
 * Draw or type a signature or initials. Drawn strokes are points on a
 * 1000 by 300 pad with the origin top left, within the server's limits of
 * 50 strokes and 2000 points. The model is `undefined` until there is
 * something to adopt.
 */
const props = defineProps<{ label: string; typedDefault?: string }>()
const model = defineModel<SigningSignature | undefined>({ required: true })

type Mode = 'drawn' | 'typed'
const mode = ref<Mode>(model.value?.kind ?? 'drawn')
const modes = [
  { value: 'drawn' as const, label: 'Draw' },
  { value: 'typed' as const, label: 'Type' },
]
const typed = ref(model.value?.kind === 'typed' ? model.value.name : (props.typedDefault ?? ''))
// Strokes change on every pointer move, so they stay out of Vue's
// reactivity; `points` and `strokeCount` are what the template reads.
let strokes: [number, number][][] =
  model.value?.kind === 'drawn' ? model.value.strokes.map((s) => s.map(([x, y]) => [x!, y!])) : []
const pad = ref<HTMLCanvasElement>()
let current: [number, number][] | null = null
const points = ref(0)
const strokeCount = ref(0)

function counted() {
  points.value = strokes.reduce((sum, stroke) => sum + stroke.length, 0)
  strokeCount.value = strokes.length
}
counted()

const full = computed(() => strokeCount.value >= MAX_STROKES || points.value >= MAX_POINTS)

function emitModel() {
  if (mode.value === 'typed') {
    const name = typed.value.trim()
    model.value = name ? { kind: 'typed', name } : undefined
  } else {
    const drawn = strokes.filter((stroke) => stroke.length > 0).map((stroke) => [...stroke])
    model.value = points.value >= 2 ? { kind: 'drawn', strokes: drawn } : undefined
  }
}

watch([mode, typed], emitModel)

function context() {
  const ctx = pad.value?.getContext('2d')
  if (!ctx || !pad.value) return undefined
  // The ink follows the theme's text color, read from the pad's classes.
  ctx.strokeStyle = getComputedStyle(pad.value).color
  ctx.lineWidth = 6
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  return ctx
}

function redraw() {
  const ctx = context()
  if (!ctx) return
  ctx.clearRect(0, 0, PAD_WIDTH, PAD_HEIGHT)
  for (const stroke of strokes) {
    ctx.beginPath()
    stroke.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)))
    ctx.stroke()
  }
}

onMounted(redraw)
watch(mode, (value) => value === 'drawn' && requestAnimationFrame(redraw))

function point(event: PointerEvent): [number, number] {
  const box = pad.value!.getBoundingClientRect()
  // Pointer capture keeps reporting moves outside the pad; the server
  // refuses points off it, so they're pinned to the edge.
  const clamp = (value: number, max: number) => Math.min(max, Math.max(0, Math.round(value)))
  return [
    clamp(((event.clientX - box.left) / box.width) * PAD_WIDTH, PAD_WIDTH),
    clamp(((event.clientY - box.top) / box.height) * PAD_HEIGHT, PAD_HEIGHT),
  ]
}

function down(event: PointerEvent) {
  if (full.value) return
  pad.value?.setPointerCapture(event.pointerId)
  current = [point(event)]
  strokes.push(current)
  counted()
}

function move(event: PointerEvent) {
  if (!current || points.value >= MAX_POINTS) return
  const [x, y] = point(event)
  const [px, py] = current[current.length - 1]!
  if (Math.abs(x - px) + Math.abs(y - py) < 3) return
  current.push([x, y])
  points.value++
  const ctx = context()
  ctx?.beginPath()
  ctx?.moveTo(px, py)
  ctx?.lineTo(x, y)
  ctx?.stroke()
}

function up() {
  current = null
  emitModel()
}

function clear() {
  strokes = []
  counted()
  redraw()
  emitModel()
}
</script>

<template>
  <div class="flex flex-col gap-3">
    <div class="flex items-center justify-between gap-2">
      <SegmentedControl v-model="mode" :options="modes" :label="`How to give your ${label}`" />
      <button
        v-if="mode === 'drawn'"
        type="button"
        class="btn btn-ghost btn-sm"
        :disabled="strokeCount === 0"
        @click="clear"
      >
        Clear
      </button>
    </div>
    <div v-show="mode === 'drawn'" class="flex flex-col gap-1.5">
      <canvas
        ref="pad"
        :width="PAD_WIDTH"
        :height="PAD_HEIGHT"
        role="img"
        :aria-label="`Draw your ${label} here`"
        data-testid="signature-pad"
        class="block h-auto w-full cursor-crosshair touch-none rounded-field border border-field-border bg-field text-base-content"
        @pointerdown="down"
        @pointermove="move"
        @pointerup="up"
        @pointercancel="up"
      ></canvas>
      <span class="text-xs text-base-content/70">
        <template v-if="full">That's as much as a drawing can hold. Clear to start over.</template>
        <template v-else>Draw with your mouse, finger or pen.</template>
      </span>
    </div>
    <Field v-if="mode === 'typed'" :label="`Type your ${label}`">
      <input
        v-model="typed"
        type="text"
        class="input w-full font-serif text-lg italic"
        :maxlength="MAX_TYPED"
        autocomplete="name"
      />
    </Field>
  </div>
</template>
