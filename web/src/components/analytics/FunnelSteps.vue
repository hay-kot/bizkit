<script setup lang="ts">
import { computed } from 'vue'
import type { RouteLocationRaw } from 'vue-router'

import { formatNumber, formatPercent } from '@/lib/format'
import type { StepResult } from '@/queries/analytics'

import { describeStep } from './funnelDraft'

/**
 * A funnel's results: one bar per step, scaled to the first step. With
 * `droppedLink`, each drop-off count links to the people who dropped there.
 */
const props = defineProps<{
  steps: StepResult[]
  droppedLink?: (step: number) => RouteLocationRaw
  selected?: number
}>()

const rows = computed(() => {
  const first = Math.max(1, props.steps[0]?.people ?? 0)
  return props.steps.map((result, i) => {
    const previous = props.steps[i - 1]
    return {
      key: `${i}:${result.step.kind}:${result.step.match}`,
      number: i + 1,
      label: describeStep(result.step),
      people: result.people,
      max: first,
      fromFirst: formatPercent(result.from_first),
      fromPrevious: previous ? formatPercent(result.from_previous) : undefined,
      dropped: previous ? Math.max(0, previous.people - result.people) : undefined,
    }
  })
})
</script>

<template>
  <ol class="flex flex-col gap-5" aria-label="Funnel steps">
    <li v-for="row in rows" :key="row.key" class="flex flex-col gap-1.5">
      <div class="flex flex-wrap items-baseline gap-x-4 gap-y-1 text-sm">
        <span class="font-medium">
          <span class="text-base-content/70 tabular-nums">{{ row.number }}</span>
          {{ row.label }}
        </span>
        <span class="font-medium tabular-nums">{{ formatNumber(row.people) }} people</span>
        <span class="text-base-content/70 tabular-nums">{{ row.fromFirst }} of step 1</span>
        <span v-if="row.fromPrevious" class="text-base-content/70 tabular-nums">
          {{ row.fromPrevious }} from step {{ row.number - 1 }}
        </span>
        <template v-if="row.dropped">
          <RouterLink
            v-if="droppedLink"
            :to="droppedLink(row.number)"
            class="link tabular-nums link-hover"
            :class="selected === row.number ? 'font-medium text-primary' : 'text-base-content/70'"
            :aria-current="selected === row.number ? 'true' : undefined"
          >
            {{ formatNumber(row.dropped) }} dropped off
          </RouterLink>
          <span v-else class="text-base-content/70 tabular-nums">
            {{ formatNumber(row.dropped) }} dropped off
          </span>
        </template>
      </div>
      <progress
        class="progress h-3 w-full text-primary"
        :value="row.people"
        :max="row.max"
        :aria-label="`Step ${row.number}: ${row.fromFirst} of step 1`"
      ></progress>
    </li>
  </ol>
</template>
