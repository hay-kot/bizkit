<script setup lang="ts">
import { computed, ref } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import BarList, { type BarListItem } from '@/components/ui/BarList.vue'
import DetailPanel from '@/components/ui/DetailPanel.vue'
import PageSection from '@/components/ui/PageSection.vue'
import Select from '@/components/ui/Select.vue'
import { formatPercent } from '@/lib/format'
import type { DayRange } from '@/lib/range'
import { type BreakdownRow, type Dimension, useBreakdown } from '@/queries/analytics'

/**
 * One report panel: a ranked list for a dimension, with a switch when the
 * panel covers several dimensions, and "Show all" in a side panel.
 */
const props = withDefaults(
  defineProps<{
    workspaceId: string
    projectId: string
    range: DayRange
    title: string
    dimensions: { value: Dimension; label: string }[]
    /** What an empty value means, such as "Direct / none". */
    blank?: string
    /** Shown under the list when every row is blank, such as how to fill it. */
    blankHint?: string
  }>(),
  { blank: '(none)', blankHint: undefined },
)

const first = props.dimensions[0]
const dimension = ref<Dimension>(first ? first.value : 'page')
const { data, isPending, error } = useBreakdown(
  () => props.workspaceId,
  () => props.projectId,
  () => props.range,
  dimension,
)

const labels = computed(() => {
  switch (dimension.value) {
    case 'page':
      return { value: 'Visitors', secondary: 'Pageviews' }
    case 'event':
      return { value: 'Events', secondary: 'Visitors' }
    case 'goal':
      return { value: 'Conversions', secondary: undefined }
    default:
      return { value: 'Visitors', secondary: undefined }
  }
})

function toItem(row: BreakdownRow): BarListItem {
  const label = row.value === '' ? props.blank : row.value
  switch (dimension.value) {
    case 'page':
      return { key: row.value, label, value: row.visitors, secondary: row.pageviews }
    case 'event':
      return { key: row.value, label, value: row.events, secondary: row.visitors }
    case 'goal':
      return {
        key: row.goal_id ?? row.value,
        label: `${label} (${formatPercent(row.conversion_rate ?? 0)})`,
        value: row.visitors,
      }
    default:
      return { key: row.value, label, value: row.visitors }
  }
}

const items = computed(() => (data.value?.items ?? []).map(toItem))
const allBlank = computed(
  () => items.value.length > 0 && (data.value?.items ?? []).every((row) => row.value === ''),
)
const showingAll = ref(false)
</script>

<template>
  <PageSection :title="title">
    <template v-if="dimensions.length > 1" #actions>
      <Select v-model="dimension" :options="dimensions" :label="`${title} by`" size="toolbar" />
    </template>
    <ErrorAlert v-if="error" :error="error" />
    <BarList
      v-else
      :items="items"
      :value-label="labels.value"
      :secondary-label="labels.secondary"
      :loading="isPending"
      @show-all="showingAll = true"
    />
    <p v-if="allBlank && blankHint" class="text-sm text-base-content/70">{{ blankHint }}</p>
    <DetailPanel :open="showingAll" :title="title" @close="showingAll = false">
      <BarList
        :items="items"
        :value-label="labels.value"
        :secondary-label="labels.secondary"
        :limit="items.length"
      />
    </DetailPanel>
  </PageSection>
</template>
