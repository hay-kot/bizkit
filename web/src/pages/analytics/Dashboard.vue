<script setup lang="ts">
import { LayoutDashboard, Pencil, Trash2 } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import InsightTile from '@/components/analytics/InsightTile.vue'
import RangeToolbar from '@/components/analytics/RangeToolbar.vue'
import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import DetailActions from '@/components/ui/DetailActions.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import ReportGrid from '@/components/ui/ReportGrid.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { useReportRange } from '@/composables/useReportRange'
import { runAction } from '@/lib/action'
import {
  type Insight,
  useDashboard,
  useDeleteDashboard,
  useInsights,
  useUpdateDashboard,
} from '@/queries/analytics'

const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  dashboardId: string
}>()
const router = useRouter()
const { today, range } = useReportRange()
const { data: dashboard, error: dashboardError } = useDashboard(
  () => props.workspaceId,
  () => props.dashboardId,
)
const insights = useInsights(
  () => props.workspaceId,
  () => props.projectId,
)
const base = computed(() => `/w/${props.workspaceId}/p/${props.projectId}/analytics`)

// The URL's project comes from the link, the dashboard's from the server.
watch(dashboard, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(
      `/w/${props.workspaceId}/p/${loaded.project_id}/analytics/dashboards/${loaded.id}`,
    )
  }
})

/** The tiles in the dashboard's order; the range above applies to all of them. */
const tiles = computed((): Insight[] => {
  const byId = new Map((insights.data.value?.items ?? []).map((i) => [i.id, i]))
  return (dashboard.value?.insight_ids ?? []).flatMap((id) => byId.get(id) ?? [])
})

const update = useUpdateDashboard()
function setTiles(ids: string[], success: string) {
  return runAction({
    run: () =>
      update.mutateAsync({
        path: { workspace_id: props.workspaceId, dashboard_id: props.dashboardId },
        body: { insight_ids: ids },
      }),
    success,
  })
}
/**
 * Moves a tile among the shown tiles. Saved IDs the insights list doesn't
 * have yet keep their place at the end, so a stale list never drops them.
 */
function move(insight: Insight, by: -1 | 1) {
  if (update.isLoading.value) return
  const shown = tiles.value.map((tile) => tile.id)
  const at = shown.indexOf(insight.id)
  if (at < 0 || at + by < 0 || at + by >= shown.length) return
  shown.splice(at, 1)
  shown.splice(at + by, 0, insight.id)
  const hidden = (dashboard.value?.insight_ids ?? []).filter((id) => !shown.includes(id))
  void setTiles([...shown, ...hidden], by < 0 ? 'Moved up' : 'Moved down')
}
function removeTile(insight: Insight) {
  const ids = (dashboard.value?.insight_ids ?? []).filter((id) => id !== insight.id)
  void setTiles(ids, `Removed ${insight.name}; the insight stays saved`)
}

const renaming = ref(false)
const name = ref('')
function openRename() {
  name.value = dashboard.value?.name ?? ''
  update.reset()
  renaming.value = true
}
async function onRename() {
  await update.mutateAsync({
    path: { workspace_id: props.workspaceId, dashboard_id: props.dashboardId },
    body: { name: name.value },
  })
  renaming.value = false
}

const remove = useDeleteDashboard()
function onDelete() {
  const label = dashboard.value?.name ?? 'this dashboard'
  return runAction({
    confirm: {
      title: `Delete ${label}?`,
      message: 'This removes the dashboard. Its insights stay saved.',
      confirmLabel: 'Delete dashboard',
    },
    run: async () => {
      await remove.mutateAsync({
        path: { workspace_id: props.workspaceId, dashboard_id: props.dashboardId },
      })
      return true
    },
    success: `Deleted ${label}`,
  }).then(async (deleted) => {
    if (deleted) await router.push(`${base.value}/insights`)
  })
}
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    :title="dashboard?.name ?? 'Dashboard'"
    description="The range here applies to every insight on the dashboard."
  >
    <template #actions>
      <DetailActions>
        <template #menu>
          <MenuItem @select="openRename"
            ><Pencil class="size-4" aria-hidden="true" />Rename</MenuItem
          >
          <MenuItem danger @select="onDelete">
            <Trash2 class="size-4" aria-hidden="true" />Delete dashboard
          </MenuItem>
        </template>
      </DetailActions>
    </template>
    <ErrorAlert v-if="dashboardError" :error="dashboardError" />
    <ErrorAlert v-else-if="insights.error.value" :error="insights.error.value" />
    <template v-if="!dashboardError && !insights.error.value">
      <RangeToolbar v-model:range="range" :today="today" />
      <SkeletonRows v-if="!dashboard || insights.isPending.value" :rows="6" />
      <EmptyState
        v-else-if="!tiles.length"
        :icon="LayoutDashboard"
        title="No insights on this dashboard"
        message="Open an insight and pick Add to dashboard."
      >
        <RouterLink :to="`${base}/insights`" class="btn btn-sm">Go to insights</RouterLink>
      </EmptyState>
      <ReportGrid v-else>
        <InsightTile
          v-for="(insight, at) in tiles"
          :key="insight.id"
          :workspace-id="workspaceId"
          :insight="insight"
          :range="range"
          :to="`${base}/insights/${insight.id}`"
          :first="at === 0"
          :last="at === tiles.length - 1"
          @up="move(insight, -1)"
          @down="move(insight, 1)"
          @remove="removeTile(insight)"
        />
      </ReportGrid>
    </template>

    <FormPanel
      :open="renaming"
      variant="dialog"
      title="Rename dashboard"
      submit-label="Rename"
      :pending="update.isLoading.value"
      :error="update.error.value"
      @close="renaming = false"
      @submit="onRename"
    >
      <Field label="Name">
        <input v-model="name" name="dashboard-name" class="input w-full" required maxlength="100" />
      </Field>
    </FormPanel>
  </AnalyticsShell>
</template>
