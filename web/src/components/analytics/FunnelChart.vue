<script setup lang="ts">
import { useElementSize } from '@vueuse/core'
import { computed, ref, useId } from 'vue'
import type { RouteLocationRaw } from 'vue-router'

import { barPath } from '@/lib/chart'
import { formatNumber, formatPercent } from '@/lib/format'
import type { StepResult } from '@/queries/analytics'

import { CHIP_MIN_BAND, funnelBars } from './funnel'

/**
 * A funnel's results as bars scaled to step 1, with the share each step
 * lost hatched above it. A chip between bars gives the step-to-step
 * conversion and the drop-off; with `droppedLink` the drop-off links to
 * the people who dropped there. When bars are too narrow for chips, the
 * drop-off moves under its step's name.
 */
const props = defineProps<{
  steps: StepResult[]
  droppedLink?: (step: number) => RouteLocationRaw
  selected?: number
}>()

const HEIGHT = 260
// The same pads as TimeSeriesChart, so a funnel lines up with the charts
// around it; the label row under the plot matches them with pl-11 pr-3.
const PAD = { top: 24, right: 12, bottom: 4, left: 44 }
const BADGE_WIDTH = 64
const CHIP_WIDTH = 112
const CHIP_HEIGHT = 44
const TICKS = [0, 0.25, 0.5, 0.75, 1]
const hatchId = useId()

const box = ref<HTMLElement>()
const { width: measured } = useElementSize(box)
// Before the first layout (and in tests) the box has no width yet.
const width = computed(() => measured.value || 600)
const plotWidth = computed(() => Math.max(1, width.value - PAD.left - PAD.right))
const plotHeight = HEIGHT - PAD.top - PAD.bottom
const baseline = PAD.top + plotHeight

const entered = computed(() => props.steps[0]?.people ?? 0)
const bars = computed(() =>
  funnelBars(props.steps, plotWidth.value, plotHeight).map((bar) => {
    const x = PAD.left + bar.x
    const top = PAD.top + bar.top
    const lostTop = bar.lostTop === undefined ? undefined : PAD.top + bar.lostTop
    const lost = lostTop !== undefined && top - lostTop > 0
    return {
      ...bar,
      key: `${bar.number}:${bar.label}`,
      kept: barPath(x, top, bar.width, baseline - top, lost ? 0 : 3),
      lost: lost ? barPath(x, lostTop, bar.width, top - lostTop, 3) : '',
      badgeX: x + bar.width / 2 - BADGE_WIDTH / 2,
      badgeY: top - 22,
      chipX: PAD.left + bar.bandX - CHIP_WIDTH / 2,
    }
  }),
)
const chips = computed(() => (bars.value[0]?.bandWidth ?? 0) >= CHIP_MIN_BAND)

function y(share: number): number {
  return baseline - share * plotHeight
}

const summary = computed(() => {
  const last = bars.value[bars.value.length - 1]
  if (!last) return 'Funnel with no steps'
  if (!entered.value) return `Funnel of ${bars.value.length} steps: no one entered in the range`
  return `Funnel of ${bars.value.length} steps: ${formatNumber(entered.value)} people entered, ${formatNumber(last.people)} finished, ${formatPercent(last.fromFirst)}`
})
</script>

<template>
  <figure ref="box" class="relative flex w-full flex-col gap-2">
    <figcaption class="sr-only">{{ summary }}</figcaption>
    <svg
      :width="width"
      :height="HEIGHT"
      :viewBox="`0 0 ${width} ${HEIGHT}`"
      class="block overflow-visible"
    >
      <defs>
        <pattern
          :id="hatchId"
          width="6"
          height="6"
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(45)"
        >
          <rect width="6" height="6" class="fill-chart-1/10" />
          <line x1="0" y1="0" x2="0" y2="6" stroke-width="2" class="stroke-chart-1/40" />
        </pattern>
      </defs>
      <g aria-hidden="true">
        <g v-for="tick in TICKS" :key="tick">
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
            {{ formatPercent(tick) }}
          </text>
        </g>
        <template v-for="bar in bars" :key="bar.key">
          <path v-if="bar.lost" :d="bar.lost" :fill="`url(#${hatchId})`" data-part="lost" />
          <path v-if="bar.kept" :d="bar.kept" class="fill-chart-1" data-part="kept" />
          <foreignObject
            v-if="entered"
            :x="bar.badgeX"
            :y="bar.badgeY"
            :width="BADGE_WIDTH"
            height="20"
            class="pointer-events-none overflow-visible"
          >
            <div class="flex justify-center">
              <span
                class="rounded-field border border-chart-1 bg-base-100 px-1 text-xs leading-4 font-medium tabular-nums"
                >{{ formatPercent(bar.fromFirst) }}</span
              >
            </div>
          </foreignObject>
        </template>
      </g>
      <template v-if="chips">
        <template v-for="bar in bars" :key="`chip-${bar.key}`">
          <!-- HTML inside the SVG, so the chip is placed with attributes rather than an inline style. -->
          <foreignObject
            v-if="bar.fromPrevious !== undefined"
            :x="bar.chipX"
            :y="PAD.top + plotHeight / 2 - CHIP_HEIGHT / 2"
            :width="CHIP_WIDTH"
            :height="CHIP_HEIGHT"
            class="overflow-visible"
            data-part="chip"
          >
            <div class="flex justify-center">
              <div
                class="flex flex-col items-center rounded-box border border-base-300 bg-raised px-2 py-1 text-xs whitespace-nowrap shadow-sm"
              >
                <span class="font-medium tabular-nums"
                  ><span aria-hidden="true">→ </span>{{ formatPercent(bar.fromPrevious) }}</span
                >
                <RouterLink
                  v-if="droppedLink && bar.dropped"
                  :to="droppedLink(bar.number)"
                  class="link text-error tabular-nums"
                  :class="{ 'font-semibold': selected === bar.number }"
                  :aria-current="selected === bar.number ? 'true' : undefined"
                  >{{ formatNumber(bar.dropped ?? 0) }} dropped off</RouterLink
                >
                <span v-else class="text-error tabular-nums"
                  >{{ formatNumber(bar.dropped ?? 0) }} dropped off</span
                >
              </div>
            </div>
          </foreignObject>
        </template>
      </template>
    </svg>
    <p
      v-if="!entered"
      class="absolute inset-x-0 top-0 flex h-64 items-center justify-center text-sm text-base-content/70"
    >
      No one entered this funnel in the range
    </p>
    <ol class="flex pr-3 pl-11" aria-label="Funnel steps">
      <li
        v-for="bar in bars"
        :key="bar.key"
        class="flex min-w-0 flex-1 flex-col items-center gap-0.5 px-1 text-center"
        :class="chips ? 'text-sm' : 'text-xs'"
      >
        <span class="w-full truncate font-medium" :title="bar.label">
          <span class="text-base-content/70 tabular-nums">{{ bar.number }}</span>
          {{ bar.label }}
        </span>
        <span class="text-base-content/70 tabular-nums">{{ formatNumber(bar.people) }} people</span>
        <span v-if="bar.fromPrevious !== undefined" class="sr-only"
          >{{ formatPercent(bar.fromPrevious) }} from step {{ bar.number - 1 }},
          {{ formatPercent(bar.fromFirst) }} of step 1</span
        >
        <template v-if="!chips && bar.dropped">
          <RouterLink
            v-if="droppedLink"
            :to="droppedLink(bar.number)"
            class="link text-error tabular-nums"
            :class="{ 'font-semibold': selected === bar.number }"
            :aria-current="selected === bar.number ? 'true' : undefined"
            >{{ formatNumber(bar.dropped) }} dropped off</RouterLink
          >
          <span v-else class="text-error tabular-nums"
            >{{ formatNumber(bar.dropped) }} dropped off</span
          >
        </template>
      </li>
    </ol>
  </figure>
</template>
