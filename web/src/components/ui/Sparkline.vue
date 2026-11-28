<script setup lang="ts">
import { useElementSize } from '@vueuse/core'
import { computed, ref } from 'vue'

import { type Coord, monotoneSegments, pathBetween } from '@/lib/chart'

/**
 * A small line with no axes, for a KPI tile: the shape of `values`, scaled
 * from 0 to their max. `partialFrom` marks the first bucket that hasn't
 * ended; the line turns dotted from the point before it. Decorative: the
 * tile says the numbers, so screen readers skip it.
 */
const props = withDefaults(
  defineProps<{ values: number[]; partialFrom?: number; height?: number }>(),
  { partialFrom: undefined, height: 40 },
)

// Room for the stroke at the top and bottom edges.
const PAD = 2
const box = ref<HTMLElement>()
const { width: measured } = useElementSize(box)
// Before the first layout (and in tests) the box has no width yet.
const width = computed(() => measured.value || 200)

const coords = computed((): Coord[] => {
  const count = props.values.length
  const max = Math.max(0, ...props.values) || 1
  const span = props.height - PAD * 2
  return props.values.map((value, i) => [
    count === 1 ? width.value / 2 : (i / (count - 1)) * width.value,
    PAD + span - (value / max) * span,
  ])
})

const paths = computed(() => {
  const points = coords.value
  const last = points.length - 1
  const segments = monotoneSegments(points)
  const from = props.partialFrom
  const split = from === undefined || from > last ? last : Math.max(0, from - 1)
  const whole = pathBetween(points, segments, 0, last)
  return {
    solid: split > 0 ? pathBetween(points, segments, 0, split) : '',
    partial: split < last ? pathBetween(points, segments, split, last) : '',
    area: last > 0 ? `${whole} L${width.value},${props.height} L0,${props.height} Z` : '',
    dot: last === 0 ? points[0] : undefined,
  }
})
</script>

<template>
  <div ref="box" class="w-full" aria-hidden="true">
    <svg
      :width="width"
      :height="height"
      :viewBox="`0 0 ${width} ${height}`"
      class="block overflow-visible"
    >
      <path v-if="paths.area" :d="paths.area" class="fill-chart-1/10" data-part="area" />
      <path
        v-if="paths.solid"
        :d="paths.solid"
        fill="none"
        stroke-linejoin="round"
        stroke-linecap="round"
        class="stroke-chart-1 stroke-2"
        data-part="solid"
      />
      <path
        v-if="paths.partial"
        :d="paths.partial"
        fill="none"
        stroke-linecap="round"
        stroke-dasharray="1 4"
        class="stroke-chart-1 stroke-2"
        data-part="partial"
      />
      <circle
        v-if="paths.dot"
        :cx="paths.dot[0]"
        :cy="paths.dot[1]"
        r="2"
        class="fill-chart-1"
        data-part="dot"
      />
    </svg>
  </div>
</template>
