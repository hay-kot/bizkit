<script setup lang="ts">
import { useIntervalFn, useNow } from '@vueuse/core'
import { computed, ref } from 'vue'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import BreakdownPanel from '@/components/analytics/BreakdownPanel.vue'
import {
  METRICS,
  type OverviewMetric,
  overviewChart,
  overviewTiles,
} from '@/components/analytics/overview'
import RangeToolbar from '@/components/analytics/RangeToolbar.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import PageSection from '@/components/ui/PageSection.vue'
import ReportGrid from '@/components/ui/ReportGrid.vue'
import SegmentedControl from '@/components/ui/SegmentedControl.vue'
import StatTiles from '@/components/ui/StatTiles.vue'
import TimeSeriesChart from '@/components/ui/TimeSeriesChart.vue'
import { useReportRange } from '@/composables/useReportRange'
import { rangeLength, rangeToQuery } from '@/lib/range'
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
// The open bucket moves on without a reload.
const now = useNow({ scheduler: (tick) => useIntervalFn(tick, 60_000) })

const tiles = computed(() =>
  overviewTiles(summary.value, compare.value, rangeLength(range.value), now.value.getTime()),
)

const metric = ref<OverviewMetric>('visitors')
const metricOptions = (Object.keys(METRICS) as OverviewMetric[]).map((value) => ({
  value,
  label: METRICS[value].label,
}))
const chart = computed(() =>
  overviewChart(summary.value, metric.value, compare.value, now.value.getTime()),
)

/** The events explorer over this range, for one event name or all. */
function explore(name?: string) {
  return {
    path: `/w/${props.workspaceId}/p/${props.projectId}/analytics/events`,
    query: { ...(name ? { name } : {}), ...rangeToQuery(range.value) },
  }
}

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
      <PageSection :title="METRICS[metric].label" :description="METRICS[metric].description">
        <template #actions>
          <SegmentedControl v-model="metric" :options="metricOptions" label="Chart metric" />
          <RouterLink :to="explore('pageview')" class="btn h-9 btn-ghost btn-sm"
            >Explore</RouterLink
          >
        </template>
        <TimeSeriesChart
          :points="chart.points"
          :series="chart.series"
          :partial-from="chart.partialFrom"
        />
      </PageSection>
    </template>
    <PageSection
      title="What people do"
      description="The most viewed pages and the most sent events in this period."
    >
      <ReportGrid>
        <BreakdownPanel
          v-bind="reportProps"
          title="Top pages"
          description="Pages with the most visitors."
          :dimensions="[{ value: 'page', label: 'Page' }]"
          :explore="explore('pageview')"
        />
        <BreakdownPanel
          v-bind="reportProps"
          title="Top events"
          description="Custom events sent the most."
          :dimensions="[{ value: 'event', label: 'Event' }]"
          :explore="explore()"
        />
      </ReportGrid>
    </PageSection>
    <ReportGrid>
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
    </ReportGrid>
  </AnalyticsShell>
</template>
