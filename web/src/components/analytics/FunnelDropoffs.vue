<script setup lang="ts">
import type { RouteLocationRaw } from 'vue-router'

import DataTable from '@/components/ui/DataTable.vue'
import PageSection from '@/components/ui/PageSection.vue'
import { formatDateTime, formatNumber, formatTime } from '@/lib/format'
import type { Dropoff, DropoffPage, EventItem } from '@/queries/analytics'

/**
 * The people who reached the step before `step` and stopped. Identified
 * people link to their timeline; anonymous visitors show their events here.
 */
const props = defineProps<{
  step: number
  label: string
  page: DropoffPage | undefined
  loading: boolean
  error: unknown
  personPath: (personId: string) => string
  close: RouteLocationRaw
}>()

const columns = [
  { key: 'person', label: 'Person' },
  { key: 'entered', label: 'Entered' },
  { key: 'last', label: `Reached step ${props.step - 1}` },
  { key: 'events', label: 'Events' },
]

function rowKey(row: Dropoff): string {
  return `${row.person_id ?? 'anon'}:${row.entered_at}:${row.events[0]?.id ?? ''}`
}

function eventLabel(event: EventItem): string {
  return event.name === 'pageview' ? (event.path ?? 'a page') : event.name
}
</script>

<template>
  <PageSection :title="`Dropped before step ${step}: ${label}`">
    <template #description>
      <span v-if="page">
        {{ formatNumber(page.total) }}
        {{ page.total === 1 ? 'person' : 'people' }} reached step {{ step - 1 }} and stopped<span
          v-if="page.total > page.items.length"
          >; the latest {{ formatNumber(page.items.length) }} are listed</span
        >.
      </span>
    </template>
    <template #actions>
      <RouterLink :to="close" class="link text-sm link-hover">Close</RouterLink>
    </template>
    <DataTable
      :columns="columns"
      :rows="page?.items"
      :row-key="rowKey"
      :loading="loading"
      :error="error"
      :empty="{ title: 'Nobody dropped off here', message: 'Try a longer range.' }"
      label="People who dropped off"
    >
      <template #cell-person="{ row }">
        <RouterLink
          v-if="row.person_id"
          :to="personPath(row.person_id)"
          class="link font-medium link-hover"
        >
          {{ row.display ?? row.user_id }}
        </RouterLink>
        <span v-else class="text-base-content/70">Anonymous visitor</span>
      </template>
      <template #cell-entered="{ row }">
        <span class="text-sm">{{ formatDateTime(row.entered_at) }}</span>
      </template>
      <template #cell-last="{ row }">
        <span class="text-sm">{{ formatDateTime(row.last_step_at) }}</span>
      </template>
      <template #cell-events="{ row }">
        <ol class="flex flex-col gap-0.5 text-sm">
          <li v-for="event in row.events" :key="event.id" class="flex gap-2">
            <span class="shrink-0 text-base-content/70 tabular-nums">{{
              formatTime(event.ts)
            }}</span>
            <span class="truncate">{{ eventLabel(event) }}</span>
          </li>
        </ol>
      </template>
    </DataTable>
  </PageSection>
</template>
