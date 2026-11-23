<script setup lang="ts">
import { computed } from 'vue'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import BreakdownPanel from '@/components/analytics/BreakdownPanel.vue'
import RangeToolbar from '@/components/analytics/RangeToolbar.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import ReportGrid from '@/components/ui/ReportGrid.vue'
import StatTiles, { type StatTile } from '@/components/ui/StatTiles.vue'
import TimeSeriesChart, {
  type ChartPoint,
  type ChartSeries,
} from '@/components/ui/TimeSeriesChart.vue'
import { useReportRange } from '@/composables/useReportRange'
import { formatShortUtcDay, isPast } from '@/lib/format'
import { useSummary } from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const { today, range, compare } = useReportRange()
const {
  data: summary,
  isPending,
  error,
} = useSummary(
  () => props.workspaceId,
  () => props.projectId,
  range,
  compare,
)

const tiles = computed((): StatTile[] => {
  const totals = summary.value?.totals
  const previous = compare.value ? summary.value?.previous : undefined
  const tile = (label: string, key: 'visitors' | 'pageviews' | 'events' | 'conversions') => ({
    label,
    value: totals?.[key] ?? 0,
    previous: previous ? previous[key] : undefined,
  })
  return [
    tile('Unique visitors', 'visitors'),
    tile('Pageviews', 'pageviews'),
    tile('Custom events', 'events'),
    tile('Conversions', 'conversions'),
  ]
})

function pointLabel(start: string): string {
  return summary.value?.interval === 'hour'
    ? `${start.slice(11, 16)} UTC`
    : formatShortUtcDay(start)
}

const comparing = computed(() => compare.value && !!summary.value?.previous_series)
const series = computed((): ChartSeries[] =>
  comparing.value
    ? [
        { name: 'Visitors', tone: 'primary' },
        { name: 'Previous visitors', tone: 'muted' },
      ]
    : [
        { name: 'Visitors', tone: 'primary' },
        { name: 'Pageviews', tone: 'muted' },
      ],
)
const points = computed((): ChartPoint[] => {
  const previous = summary.value?.previous_series ?? []
  // Hours (or days) still ahead would plot as zeros, so the line stops at now.
  return (summary.value?.series ?? []).flatMap((point, i) =>
    isPast(point.start)
      ? [
          {
            t: point.start,
            label: pointLabel(point.start),
            values: comparing.value
              ? [point.visitors, previous[i]?.visitors ?? 0]
              : [point.visitors, point.pageviews],
          },
        ]
      : [],
  )
})

const reportProps = computed(() => ({
  workspaceId: props.workspaceId,
  projectId: props.projectId,
  range: range.value,
}))
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="setup"
    description="Visitors, sources and goals for this project's site. Days are UTC."
  >
    <RangeToolbar v-model:range="range" v-model:compare="compare" :today="today" with-compare />
    <ErrorAlert v-if="error" :error="error" />
    <template v-else>
      <StatTiles :tiles="tiles" :loading="isPending" />
      <TimeSeriesChart :points="points" :series="series" />
    </template>
    <ReportGrid>
      <BreakdownPanel
        v-bind="reportProps"
        title="Top pages"
        :dimensions="[{ value: 'page', label: 'Page' }]"
      />
      <BreakdownPanel
        v-bind="reportProps"
        title="Sources"
        blank="Direct / none"
        :dimensions="[
          { value: 'referrer', label: 'Referrer' },
          { value: 'utm_source', label: 'UTM source' },
          { value: 'utm_medium', label: 'UTM medium' },
          { value: 'utm_campaign', label: 'UTM campaign' },
        ]"
      />
      <BreakdownPanel
        v-bind="reportProps"
        title="Countries"
        blank="Unknown"
        blank-hint="Countries need a GeoIP database. Set server.modules.analytics.geoip_path to a DB-IP Lite country file."
        :dimensions="[{ value: 'country', label: 'Country' }]"
      />
      <BreakdownPanel
        v-bind="reportProps"
        title="Devices"
        blank="Unknown"
        :dimensions="[
          { value: 'device', label: 'Device type' },
          { value: 'browser', label: 'Browser' },
          { value: 'os', label: 'OS' },
        ]"
      />
      <BreakdownPanel
        v-bind="reportProps"
        title="Goals"
        :dimensions="[{ value: 'goal', label: 'Goal' }]"
      />
      <BreakdownPanel
        v-bind="reportProps"
        title="Custom events"
        :dimensions="[{ value: 'event', label: 'Event' }]"
      />
    </ReportGrid>
  </AnalyticsShell>
</template>
