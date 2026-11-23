<script setup lang="ts">
import { computed } from 'vue'

import DataTable from '@/components/ui/DataTable.vue'
import { formatNumber, formatPercent, formatShortUtcDay } from '@/lib/format'
import type { Cohort } from '@/queries/analytics'

import { shadeClass } from './retention'

/** Cohorts by first-seen week against weeks 0 to 8, each cell shaded by its share. */
const props = defineProps<{ cohorts: Cohort[] | undefined; loading: boolean; error: unknown }>()

const WEEKS = 9
const weekKeys = Array.from({ length: WEEKS }, (_, i) => `w${i}`)
const columns = [
  { key: 'week', label: 'First seen' },
  { key: 'people', label: 'People', class: 'text-right' },
  ...weekKeys.map((key, i) => ({ key, label: `Week ${i}`, class: 'w-16 text-center' })),
]

const rows = computed(() =>
  (props.cohorts ?? []).map((cohort) => ({
    key: cohort.week,
    week: `Week of ${formatShortUtcDay(cohort.week)}`,
    people: formatNumber(cohort.people),
    cells: weekKeys.map((_, i) => {
      const share = cohort.weeks[i]
      return share === null || share === undefined
        ? { text: '', spoken: 'not begun', shade: '' }
        : { text: formatPercent(share), spoken: formatPercent(share), shade: shadeClass(share) }
    }),
  })),
)
type Row = (typeof rows.value)[number]

function cell(row: Row, key: string) {
  return row.cells[Number(key.slice(1))]
}
</script>

<template>
  <DataTable
    :columns="columns"
    :rows="rows"
    :row-key="(row) => row.key"
    :loading="loading"
    :error="error"
    label="Retention by first-seen week"
    :empty="{
      title: 'No cohorts in this range',
      message: 'Cohorts start with people first seen in the range.',
    }"
  >
    <template #cell-week="{ row }"
      ><span class="text-sm">{{ row.week }}</span></template
    >
    <template #cell-people="{ row }"
      ><span class="text-sm tabular-nums">{{ row.people }}</span></template
    >
    <template v-for="key in weekKeys" :key="key" #[`cell-${key}`]="{ row }">
      <span
        class="block rounded-field px-1 py-1 text-xs tabular-nums"
        :class="cell(row, key)?.shade"
        :aria-label="cell(row, key)?.spoken"
        >{{ cell(row, key)?.text }}</span
      >
    </template>
  </DataTable>
</template>
