<script setup lang="ts">
import { computed } from 'vue'

import DataTable from '@/components/ui/DataTable.vue'
import { formatNumber, formatPercent, formatShortUtcDay } from '@/lib/format'
import { addDays } from '@/lib/range'
import type { Cohort } from '@/queries/analytics'

import { inProgress, meanRow, shadeClass } from './retention'

/**
 * Cohorts by first-seen week against weeks 0 to 8, each cell shaded by its
 * share. The first row averages every cohort over the weeks that have
 * ended; a week still running on `today` has a dashed border.
 */
const props = defineProps<{
  cohorts: Cohort[] | undefined
  today: string
  loading: boolean
  error: unknown
}>()

const WEEKS = 9
const weekKeys = Array.from({ length: WEEKS }, (_, i) => `w${i}`)
const columns = [
  { key: 'week', label: 'First seen' },
  { key: 'people', label: 'People', class: 'text-right' },
  ...weekKeys.map((key, i) => ({ key, label: `Week ${i}`, class: 'w-16 text-center' })),
]

const NOT_BEGUN = { text: '', spoken: 'not begun', shade: '', running: false }
const NOT_ENDED = { ...NOT_BEGUN, spoken: 'no cohort has finished this week' }

function shareCell(share: number, running = false) {
  const text = formatPercent(share)
  return {
    text,
    spoken: running ? `${text}, in progress` : text,
    shade: shadeClass(share),
    running,
  }
}

const rows = computed(() => {
  const cohorts = props.cohorts ?? []
  if (!cohorts.length) return []
  const mean = {
    key: 'all',
    week: 'All cohorts',
    people: formatNumber(cohorts.reduce((sum, cohort) => sum + cohort.people, 0)),
    total: true,
    cells: meanRow(cohorts, props.today, WEEKS).map((share) =>
      share === null ? NOT_ENDED : shareCell(share),
    ),
  }
  return [
    mean,
    ...cohorts.map((cohort) => ({
      key: cohort.week,
      // No year: it would push week 8 out of view at laptop widths.
      week: `${formatShortUtcDay(cohort.week)} to ${formatShortUtcDay(addDays(cohort.week, 6))}`,
      people: formatNumber(cohort.people),
      total: false,
      cells: weekKeys.map((_, i) => {
        const share = cohort.weeks[i]
        return share === null || share === undefined
          ? NOT_BEGUN
          : shareCell(share, inProgress(cohort.week, i, props.today))
      }),
    })),
  ]
})
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
      ><span class="text-sm whitespace-nowrap" :class="{ 'font-medium': row.total }">{{
        row.week
      }}</span></template
    >
    <template #cell-people="{ row }"
      ><span class="text-sm tabular-nums" :class="{ 'font-medium': row.total }">{{
        row.people
      }}</span></template
    >
    <template v-for="key in weekKeys" :key="key" #[`cell-${key}`]="{ row }">
      <span
        class="block rounded-field px-1 py-1 text-xs tabular-nums"
        :class="[
          cell(row, key)?.shade,
          {
            'font-medium': row.total,
            'border border-dashed border-base-content/40': cell(row, key)?.running,
          },
        ]"
        :aria-label="cell(row, key)?.spoken"
        :data-running="cell(row, key)?.running || undefined"
        >{{ cell(row, key)?.text }}</span
      >
    </template>
  </DataTable>
</template>
