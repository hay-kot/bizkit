<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import IdentityNeeded from '@/components/analytics/IdentityNeeded.vue'
import RangeToolbar from '@/components/analytics/RangeToolbar.vue'
import RetentionTable from '@/components/analytics/RetentionTable.vue'
import Select from '@/components/ui/Select.vue'
import { useReportRange } from '@/composables/useReportRange'
import { useEventStats, useRetention } from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const route = useRoute()
const router = useRouter()
// The default covers the last eight weekly cohorts with room to spare.
const { today, range } = useReportRange('90d')

const returnEvent = computed({
  get: () => (typeof route.query.event === 'string' ? route.query.event : ''),
  set: (event: string) =>
    void router.replace({ query: { ...route.query, event: event || undefined } }),
})
const retention = useRetention(
  () => props.workspaceId,
  () => props.projectId,
  range,
  () => returnEvent.value || null,
)
const names = useEventStats(
  () => props.workspaceId,
  () => props.projectId,
  range,
  { name: null, props: [] },
)
const options = computed(() => [
  { value: '', label: 'Any event' },
  ...(names.data.value?.names ?? []).map((n) => ({ value: n.name, label: n.name })),
  { value: 'pageview', label: 'pageview' },
])
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    title="Retention"
    description="People by the week they were first seen, and the share who came back each week after. Weeks start on Monday, UTC."
  >
    <template #default="{ site }">
      <RangeToolbar v-model:range="range" :today="today">
        <Select v-model="returnEvent" :options="options" label="Return event" size="sm" />
      </RangeToolbar>
      <IdentityNeeded
        v-if="
          site.identity_mode === 'cookieless' &&
          !retention.isPending.value &&
          !retention.data.value?.cohorts.length
        "
        :workspace-id="workspaceId"
        :project-id="projectId"
        what="Retention"
      />
      <RetentionTable
        v-else
        :cohorts="retention.data.value?.cohorts"
        :loading="retention.isPending.value"
        :error="retention.error.value"
      />
    </template>
  </AnalyticsShell>
</template>
