<script setup lang="ts">
import { useElementSize } from '@vueuse/core'
import { computed, ref } from 'vue'

import { niceTicks, spreadIndexes } from '@/lib/chart'
import { formatCompactNumber, formatNumber } from '@/lib/format'

export interface ChartPoint {
  /** The bucket's start, as the API gives it. */
  t: string
  /** What the axis and the tooltip show for the bucket. */
  label: string
  /** One value per series, in series order. */
  values: number[]
}

export interface ChartSeries {
  name: string
  /**
   * `primary` is a solid line in the next chart color (`chart-1` to
   * `chart-4`), the first one with an area; `muted` a faint dashed line.
   */
  tone: 'primary' | 'muted'
}

/**
 * A line chart over time, drawn as SVG. Hover, or focus and use the arrow
 * keys, to read a point. Screen readers get a summary and a hidden table.
 * With `fill` the plot grows to the space its flex parent gives it, at
 * least 12rem, and `height` only applies before the first layout.
 */
const props = withDefaults(
  defineProps<{ points: ChartPoint[]; series: ChartSeries[]; height?: number; fill?: boolean }>(),
  { height: 240, fill: false },
)

// Static class strings, so Tailwind sees every one. Past four series the
// colors repeat.
const PALETTE = [
  { line: 'stroke-chart-1', dot: 'fill-chart-1', swatch: 'bg-chart-1', key: 'border-chart-1' },
  { line: 'stroke-chart-2', dot: 'fill-chart-2', swatch: 'bg-chart-2', key: 'border-chart-2' },
  { line: 'stroke-chart-3', dot: 'fill-chart-3', swatch: 'bg-chart-3', key: 'border-chart-3' },
  { line: 'stroke-chart-4', dot: 'fill-chart-4', swatch: 'bg-chart-4', key: 'border-chart-4' },
] as const
const MUTED = {
  line: 'stroke-base-content/40',
  dot: 'fill-base-content/40',
  swatch: 'bg-base-content/40',
  key: 'border-dashed border-base-content/40',
}

const tones = computed(() => {
  let next = 0
  return props.series.map((s) =>
    s.tone === 'muted' ? MUTED : (PALETTE[next++ % PALETTE.length] ?? PALETTE[0]),
  )
})

const PAD = { top: 12, right: 12, bottom: 28, left: 44 }
const TOOLTIP_WIDTH = 200
const TOOLTIP_HEIGHT = 96

const box = ref<HTMLElement>()
const { width: measured } = useElementSize(box)
// Before the first layout (and in tests) the box has no width yet.
const width = computed(() => measured.value || 600)
// In fill mode the SVG sits absolutely in its box, so the box's height comes
// from the layout and never from the SVG it sizes.
const plotBox = ref<HTMLElement>()
const { height: measuredHeight } = useElementSize(plotBox)
const drawHeight = computed(() => (props.fill && measuredHeight.value) || props.height)
const plotWidth = computed(() => Math.max(1, width.value - PAD.left - PAD.right))
const plotHeight = computed(() => Math.max(1, drawHeight.value - PAD.top - PAD.bottom))
const baseline = computed(() => PAD.top + plotHeight.value)

const ticks = computed(() => {
  const max = Math.max(0, ...props.points.flatMap((p) => p.values))
  return max > 0 ? niceTicks(max) : [0]
})
const top = computed(() => ticks.value[ticks.value.length - 1] || 1)

function x(index: number): number {
  const count = props.points.length
  if (count <= 1) return PAD.left + plotWidth.value / 2
  return PAD.left + (index / (count - 1)) * plotWidth.value
}

function y(value: number): number {
  return baseline.value - (value / top.value) * plotHeight.value
}

const lines = computed(() =>
  props.series.map((s, seriesIndex) => {
    const coords = props.points.map((p, i) => [x(i), y(p.values[seriesIndex] ?? 0)] as const)
    const line = coords.map(([px, py], i) => `${i ? 'L' : 'M'}${px},${py}`).join(' ')
    const first = coords[0]
    const last = coords[coords.length - 1]
    const area =
      seriesIndex === 0 && s.tone === 'primary' && first && last
        ? `${line} L${last[0]},${baseline.value} L${first[0]},${baseline.value} Z`
        : undefined
    return { ...s, line, area }
  }),
)

const xLabels = computed(() => {
  const indexes = spreadIndexes(props.points.length)
  return indexes.map((i, n) => ({
    i,
    label: props.points[i]?.label ?? '',
    anchor:
      indexes.length === 1
        ? 'middle'
        : n === 0
          ? 'start'
          : n === indexes.length - 1
            ? 'end'
            : 'middle',
  }))
})

const active = ref<number>()
const activePoint = computed(() =>
  active.value === undefined ? undefined : props.points[active.value],
)
const activeText = computed(() => {
  const point = activePoint.value
  if (!point) return ''
  const values = props.series.map((s, i) => `${s.name} ${formatNumber(point.values[i] ?? 0)}`)
  return `${point.label}: ${values.join(', ')}`
})
const tooltipX = computed(() => {
  if (active.value === undefined) return 0
  const px = x(active.value)
  return px + 12 + TOOLTIP_WIDTH > width.value ? px - 12 - TOOLTIP_WIDTH : px + 12
})
const tooltipOnLeft = computed(() => active.value !== undefined && tooltipX.value < x(active.value))

function onPointer(event: PointerEvent) {
  const count = props.points.length
  if (!count) return
  const bounds = (event.currentTarget as SVGElement).getBoundingClientRect()
  const offset = event.clientX - bounds.left - PAD.left
  const index = count === 1 ? 0 : Math.round((offset / plotWidth.value) * (count - 1))
  active.value = Math.min(count - 1, Math.max(0, index))
}

function onKey(event: KeyboardEvent) {
  const last = props.points.length - 1
  if (last < 0) return
  const current = active.value ?? last
  const next: Record<string, number> = {
    ArrowLeft: Math.max(0, current - 1),
    ArrowRight: Math.min(last, current + 1),
    Home: 0,
    End: last,
  }
  if (event.key in next) {
    event.preventDefault()
    active.value = next[event.key]
  } else if (event.key === 'Escape') {
    active.value = undefined
  }
}

const summary = computed(() => {
  const names = props.series.map((s) => s.name).join(' and ')
  const first = props.points[0]
  const last = props.points[props.points.length - 1]
  if (!first || !last) return `${names}: no data for this range`
  const totals = props.series.map(
    (s, i) =>
      `${s.name} total ${formatNumber(props.points.reduce((sum, p) => sum + (p.values[i] ?? 0), 0))}`,
  )
  return `${names} from ${first.label} to ${last.label}. ${totals.join(', ')}.`
})
</script>

<template>
  <div ref="box" class="relative w-full" :class="{ 'flex flex-1 flex-col': fill }">
    <ul
      v-if="series.length > 1"
      class="mb-2 flex flex-wrap gap-4 text-xs text-base-content/70"
      aria-hidden="true"
    >
      <li v-for="(s, i) in series" :key="s.name" class="flex items-center gap-1.5">
        <span class="w-4 border-t-2" :class="tones[i]?.key"></span>
        {{ s.name }}
      </li>
    </ul>
    <div ref="plotBox" :class="{ 'relative min-h-48 flex-1': fill }">
      <svg
        :width="width"
        :height="drawHeight"
        :viewBox="`0 0 ${width} ${drawHeight}`"
        role="img"
        :aria-label="summary"
        :tabindex="points.length ? 0 : undefined"
        class="block overflow-visible rounded-field focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
        :class="{ 'absolute inset-0': fill }"
        @pointermove="onPointer"
        @pointerleave="active = undefined"
        @focus="active = points.length ? points.length - 1 : undefined"
        @blur="active = undefined"
        @keydown="onKey"
      >
        <g aria-hidden="true">
          <g v-for="tick in ticks" :key="tick">
            <line
              :x1="PAD.left"
              :x2="width - PAD.right"
              :y1="y(tick)"
              :y2="y(tick)"
              class="stroke-base-300"
              :stroke-dasharray="tick === 0 ? undefined : '2 4'"
            />
            <text
              :x="PAD.left - 8"
              :y="y(tick)"
              text-anchor="end"
              dominant-baseline="middle"
              class="fill-base-content/70 text-xs tabular-nums"
            >
              {{ formatCompactNumber(tick) }}
            </text>
          </g>
          <text
            v-for="tick in xLabels"
            :key="tick.i"
            :x="x(tick.i)"
            :y="drawHeight - 8"
            :text-anchor="tick.anchor"
            class="fill-base-content/70 text-xs"
          >
            {{ tick.label }}
          </text>

          <template v-if="points.length">
            <template v-for="s in lines" :key="`area-${s.name}`">
              <path v-if="s.area" :d="s.area" class="fill-chart-1/10" />
            </template>
            <path
              v-for="(s, i) in lines"
              :key="`line-${s.name}`"
              :d="s.line"
              fill="none"
              stroke-linejoin="round"
              stroke-linecap="round"
              :stroke-dasharray="s.tone === 'muted' ? '4 4' : undefined"
              class="stroke-2"
              :class="tones[i]?.line"
            />
          </template>

          <g v-if="active !== undefined && activePoint">
            <line
              :x1="x(active)"
              :x2="x(active)"
              :y1="PAD.top"
              :y2="baseline"
              class="stroke-base-content/30"
            />
            <circle
              v-for="(s, i) in series"
              :key="s.name"
              :cx="x(active)"
              :cy="y(activePoint.values[i] ?? 0)"
              r="4"
              class="stroke-base-100 stroke-2"
              :class="tones[i]?.dot"
            />
            <!-- HTML inside the SVG, so the tooltip is placed with attributes rather than an inline style. -->
            <foreignObject
              :x="tooltipX"
              :y="PAD.top"
              :width="TOOLTIP_WIDTH"
              :height="TOOLTIP_HEIGHT"
              class="pointer-events-none overflow-visible"
            >
              <div class="flex" :class="tooltipOnLeft ? 'justify-end' : 'justify-start'">
                <div
                  class="flex flex-col gap-0.5 rounded-box border border-base-300 bg-raised px-2.5 py-1.5 text-xs shadow-md"
                >
                  <span class="font-medium whitespace-nowrap">{{ activePoint.label }}</span>
                  <span
                    v-for="(s, i) in series"
                    :key="s.name"
                    class="flex items-center gap-2 whitespace-nowrap"
                  >
                    <span class="size-2 rounded-full" :class="tones[i]?.swatch"></span>
                    <span class="text-base-content/70">{{ s.name }}</span>
                    <span class="ml-auto font-medium tabular-nums">{{
                      formatNumber(activePoint.values[i] ?? 0)
                    }}</span>
                  </span>
                </div>
              </div>
            </foreignObject>
          </g>
        </g>
      </svg>
    </div>
    <p
      v-if="!points.length"
      class="absolute inset-0 flex items-center justify-center text-sm text-base-content/70"
    >
      No data for this range
    </p>
    <p class="sr-only" aria-live="polite">{{ activeText }}</p>
    <!-- Tables ignore sr-only's 1px box: the wrapper clips it, or the table
         keeps its full height and stretches the page. -->
    <div class="sr-only">
      <table>
        <caption>
          {{
            series.map((s) => s.name).join(' and ')
          }}
          over time
        </caption>
        <thead>
          <tr>
            <th scope="col">Time</th>
            <th v-for="s in series" :key="s.name" scope="col">{{ s.name }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="point in points" :key="point.t">
            <th scope="row">{{ point.label }}</th>
            <td v-for="(s, i) in series" :key="s.name">{{ formatNumber(point.values[i] ?? 0) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
