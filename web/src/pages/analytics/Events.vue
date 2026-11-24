<script setup lang="ts">
import { Activity, Lightbulb, X } from 'lucide-vue-next'
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import { linkFromExplorer } from '@/components/analytics/insightDraft'
import RangeToolbar from '@/components/analytics/RangeToolbar.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import DataTable from '@/components/ui/DataTable.vue'
import DescriptionList from '@/components/ui/DescriptionList.vue'
import DetailPanel from '@/components/ui/DetailPanel.vue'
import PageSection from '@/components/ui/PageSection.vue'
import Select from '@/components/ui/Select.vue'
import TimeSeriesChart, { type ChartPoint } from '@/components/ui/TimeSeriesChart.vue'
import { useReportRange } from '@/composables/useReportRange'
import { formatDateTime, formatNumber, formatShortUtcDay, isPast } from '@/lib/format'
import {
  type EventFilter,
  type EventItem,
  RECENT_SHOWN,
  useEventStats,
  useRecentEvents,
} from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const route = useRoute()
const router = useRouter()
const { today, range } = useReportRange()

/** The filter lives in the URL with the range: `?name=` and repeated `?prop=key=value`. */
const filter = computed((): EventFilter => {
  const name = typeof route.query.name === 'string' && route.query.name ? route.query.name : null
  const raw = route.query.prop
  const list = (Array.isArray(raw) ? raw : raw ? [raw] : []).filter(
    (p): p is string => typeof p === 'string' && p.includes('='),
  )
  return { name, props: list }
})

/** A new insight that starts from this filter. */
const saveAsInsight = computed(() => ({
  path: `/w/${props.workspaceId}/p/${props.projectId}/analytics/insights/new`,
  query: {
    ...linkFromExplorer(filter.value.name, filter.value.props),
    range: route.query.range,
    from: route.query.from,
    to: route.query.to,
  },
}))

function setFilter(next: EventFilter) {
  void router.replace({
    query: { ...route.query, name: next.name ?? undefined, prop: next.props },
  })
}

const stats = useEventStats(
  () => props.workspaceId,
  () => props.projectId,
  range,
  filter,
)
const recent = useRecentEvents(
  () => props.workspaceId,
  () => props.projectId,
  range,
  filter,
)

const nameOptions = computed(() => [
  { value: '', label: 'All custom events' },
  ...(stats.data.value?.names ?? []).map((n) => ({ value: n.name, label: n.name })),
  ...(filter.value.name && !stats.data.value?.names.some((n) => n.name === filter.value.name)
    ? [{ value: filter.value.name, label: filter.value.name }]
    : []),
])
const selectedName = computed({
  get: () => filter.value.name ?? '',
  set: (name: string) => setFilter({ ...filter.value, name: name || null }),
})

const propDraft = ref('')
const propError = ref('')
function addProp() {
  const draft = propDraft.value.trim()
  const at = draft.indexOf('=')
  if (at < 1) {
    propError.value = 'Write the filter as key=value.'
    return
  }
  propError.value = ''
  propDraft.value = ''
  if (!filter.value.props.includes(draft)) {
    setFilter({ ...filter.value, props: [...filter.value.props, draft] })
  }
}
function removeProp(prop: string) {
  setFilter({ ...filter.value, props: filter.value.props.filter((p) => p !== prop) })
}

const points = computed((): ChartPoint[] =>
  (stats.data.value?.series ?? [])
    .filter((point) => isPast(point.start))
    .map((point) => ({
      t: point.start,
      label:
        stats.data.value?.interval === 'hour'
          ? `${point.start.slice(11, 16)} UTC`
          : formatShortUtcDay(point.start),
      values: [point.count, point.users],
    })),
)

const nameColumns = [
  { key: 'name', label: 'Event' },
  { key: 'count', label: 'Count', class: 'text-right' },
  { key: 'users', label: 'Unique users', class: 'text-right' },
]
const recentColumns = [
  { key: 'time', label: 'Time' },
  { key: 'name', label: 'Event' },
  { key: 'person', label: 'Person', interactive: true },
  { key: 'page', label: 'Page', interactive: true },
  { key: 'props', label: 'Properties', interactive: true },
]

function personPath(personId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/analytics/people/${personId}`
}

function summarize(values: Record<string, unknown>): string {
  return Object.entries(values)
    .map(([key, value]) => `${key}=${typeof value === 'string' ? value : JSON.stringify(value)}`)
    .join(', ')
}

const viewing = ref<EventItem | null>(null)
const viewingFacts = computed(() => {
  const event = viewing.value
  if (!event) return []
  return [
    { key: 'name', label: 'Event', value: event.name },
    { key: 'time', label: 'Time', value: formatDateTime(event.ts) },
    { key: 'page', label: 'Page', value: event.path ?? '—' },
    { key: 'person', label: 'User ID', value: event.user_id ?? 'Anonymous' },
    { key: 'source', label: 'Source', value: event.source === 'server' ? 'Server' : 'Browser' },
  ]
})
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    title="Events"
    description="Custom events sent with bizkit.track or the server API."
  >
    <template #actions>
      <RouterLink :to="saveAsInsight" class="btn gap-1.5 btn-sm">
        <Lightbulb class="size-4" aria-hidden="true" />Save as insight
      </RouterLink>
    </template>
    <RangeToolbar v-model:range="range" :today="today">
      <Select v-model="selectedName" :options="nameOptions" label="Event name" size="toolbar" />
      <form class="flex items-center gap-2" @submit.prevent="addProp">
        <input
          v-model="propDraft"
          name="event-prop-filter"
          class="input h-9 w-48"
          placeholder="plan=pro"
          aria-label="Property filter, key=value"
        />
        <button type="submit" class="btn h-9">Add filter</button>
      </form>
    </RangeToolbar>
    <p v-if="propError" class="text-sm text-error">{{ propError }}</p>
    <div v-if="filter.props.length" class="flex flex-wrap gap-2">
      <button
        v-for="prop in filter.props"
        :key="prop"
        type="button"
        class="badge gap-1 badge-outline"
        :aria-label="`Remove filter ${prop}`"
        @click="removeProp(prop)"
      >
        {{ prop }}<X class="size-3" aria-hidden="true" />
      </button>
    </div>

    <ErrorAlert v-if="stats.error.value" :error="stats.error.value" />
    <TimeSeriesChart
      v-else
      :points="points"
      :series="[
        { name: 'Events', tone: 'primary' },
        { name: 'Unique users', tone: 'muted' },
      ]"
    />

    <PageSection title="Event names" description="Pick one to filter the page by it.">
      <DataTable
        :columns="nameColumns"
        :rows="stats.data.value?.names"
        :row-key="(n) => n.name"
        :loading="stats.isPending.value"
        :error="stats.error.value"
        :empty="{
          icon: Activity,
          title: 'No custom events in this range',
          message: 'Send one with bizkit.track(name, props) on your site.',
        }"
        @row-click="(n) => (selectedName = n.name)"
      >
        <template #cell-name="{ row }">{{ row.name }}</template>
        <template #cell-count="{ row }"
          ><span class="tabular-nums">{{ formatNumber(row.count) }}</span></template
        >
        <template #cell-users="{ row }"
          ><span class="tabular-nums">{{ formatNumber(row.users) }}</span></template
        >
      </DataTable>
    </PageSection>

    <PageSection
      title="Recent events"
      :description="`The latest ${RECENT_SHOWN}, pageviews included unless you pick a name.`"
    >
      <DataTable
        :columns="recentColumns"
        :rows="recent.data.value?.items"
        :row-key="(e) => String(e.id)"
        :loading="recent.isPending.value"
        :error="recent.error.value"
        :empty="{ title: 'No events in this range' }"
        @row-click="(e) => (viewing = e)"
      >
        <template #cell-time="{ row }"
          ><span class="text-sm">{{ formatDateTime(row.ts) }}</span></template
        >
        <template #cell-name="{ row }"
          ><span class="font-medium">{{ row.name }}</span></template
        >
        <template #cell-person="{ row }">
          <RouterLink v-if="row.person_id" :to="personPath(row.person_id)" class="link text-sm">{{
            row.user_id ?? 'Person'
          }}</RouterLink>
          <span v-else class="text-sm text-base-content/70">Anonymous</span>
        </template>
        <template #cell-page="{ row }"
          ><span class="text-sm">{{ row.path ?? '' }}</span></template
        >
        <template #cell-props="{ row }">
          <span class="line-clamp-1 text-sm text-base-content/70">{{ summarize(row.props) }}</span>
        </template>
      </DataTable>
    </PageSection>

    <DetailPanel :open="viewing !== null" title="Event" @close="viewing = null">
      <DescriptionList :items="viewingFacts" />
      <pre
        v-if="viewing"
        tabindex="0"
        class="overflow-x-auto rounded-field bg-base-200 p-3 text-sm"
      ><code>{{ JSON.stringify(viewing.props, null, 2) }}</code></pre>
    </DetailPanel>
  </AnalyticsShell>
</template>
