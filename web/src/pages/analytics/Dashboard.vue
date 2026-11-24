<script setup lang="ts">
import { useEventListener } from '@vueuse/core'
import { Check, LayoutDashboard, LayoutGrid, Pencil, Plus, Trash2, X } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { onBeforeRouteLeave, onBeforeRouteUpdate, useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import {
  hiddenTiles,
  MAX_TILES,
  switchesDashboard,
  tileRoom,
} from '@/components/analytics/dashboardEdit'
import { describeQuery } from '@/components/analytics/insightDraft'
import InsightTile from '@/components/analytics/InsightTile.vue'
import RangeToolbar from '@/components/analytics/RangeToolbar.vue'
import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import DashboardGrid from '@/components/ui/DashboardGrid.vue'
import { isTileWidth, moveItem, type TileWidth } from '@/components/ui/dashboardGrid'
import DetailActions, { type DetailAction } from '@/components/ui/DetailActions.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
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

interface EditTile {
  insight: Insight
  width: TileWidth
}

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

const insightsById = computed(
  () => new Map((insights.data.value?.items ?? []).map((i) => [i.id, i])),
)

/** The saved tiles the insights list knows, in the dashboard's order. */
const saved = computed((): EditTile[] =>
  (dashboard.value?.tiles ?? []).flatMap((tile) => {
    const insight = insightsById.value.get(tile.insight_id)
    return insight ? [{ insight, width: isTileWidth(tile.width) ? tile.width : 6 }] : []
  }),
)

const editing = ref(false)
const draft = ref<EditTile[]>([])
const tiles = computed(() => (editing.value ? draft.value : saved.value))
const gridItems = computed(() =>
  tiles.value.map((tile) => ({ id: tile.insight.id, width: tile.width, label: tile.insight.name })),
)

function layoutKey(list: EditTile[]): string {
  return list.map((tile) => `${tile.insight.id}:${tile.width}`).join(',')
}
const dirty = computed(() => editing.value && layoutKey(draft.value) !== layoutKey(saved.value))

function startEditing() {
  draft.value = saved.value.map((tile) => ({ ...tile }))
  editing.value = true
}
function cancelEditing() {
  editing.value = false
  draft.value = []
}
const update = useUpdateDashboard()
// Done sends the draft as it is when clicked; an edit made while that save
// runs would be lost when the page leaves edit mode, so the grid locks.
const saving = computed(() => update.isLoading.value)

function moveTile(from: number, to: number) {
  if (saving.value) return
  draft.value = moveItem(draft.value, from, to)
}
function setWidth(index: number, width: TileWidth) {
  const tile = draft.value[index]
  if (tile && !saving.value) tile.width = width
}
function removeTile(index: number) {
  if (saving.value) return
  draft.value = draft.value.filter((_, at) => at !== index)
}

// Tiles whose insight the list doesn't have yet keep their place at the end
// of a save, so a stale insights list never drops them.
const hidden = computed(() =>
  hiddenTiles(dashboard.value?.tiles ?? [], new Set(insightsById.value.keys())),
)

async function finishEditing() {
  if (!dirty.value) {
    cancelEditing()
    return
  }
  const keep = hidden.value
  const done = await runAction({
    run: () =>
      update.mutateAsync({
        path: { workspace_id: props.workspaceId, dashboard_id: props.dashboardId },
        body: {
          tiles: [
            ...draft.value.map((tile) => ({ insight_id: tile.insight.id, width: tile.width })),
            ...keep,
          ],
        },
      }),
    success: 'Saved the layout',
  })
  if (done) cancelEditing()
}

function confirmDiscard() {
  return runAction({
    confirm: {
      title: 'Discard layout changes?',
      message: 'The dashboard keeps its saved layout.',
      confirmLabel: 'Discard changes',
    },
    run: async () => true,
  })
}
onBeforeRouteLeave(async () => {
  if (!dirty.value) return true
  return (await confirmDiscard()) === true
})
// Another dashboard reuses this page, so its edits would carry over or drop
// silently. A range change keeps the page and the edits.
onBeforeRouteUpdate(async (to, from) => {
  if (!switchesDashboard(to, from)) return true
  if (dirty.value && (await confirmDiscard()) !== true) return false
  cancelEditing()
  return true
})
useEventListener(window, 'beforeunload', (event: BeforeUnloadEvent) => {
  if (dirty.value) event.preventDefault()
})

const adding = ref(false)
const picked = ref<string[]>([])
const addable = computed(() => {
  const on = new Set(draft.value.map((tile) => tile.insight.id))
  return (insights.data.value?.items ?? []).filter((insight) => !on.has(insight.id))
})
const room = computed(() => tileRoom(draft.value.length, hidden.value.length))
function openAdd() {
  if (saving.value) return
  picked.value = []
  adding.value = true
}
function onAdd() {
  if (saving.value) return
  const added = picked.value.flatMap((id) => {
    const insight = insightsById.value.get(id)
    return insight ? [{ insight, width: 6 as const }] : []
  })
  draft.value = [...draft.value, ...added]
  adding.value = false
}

const headerActions = computed((): DetailAction[] =>
  editing.value
    ? [
        {
          label: 'Add insight',
          icon: Plus,
          run: openAdd,
          disabled: room.value <= 0 || saving.value,
        },
        { label: 'Cancel', icon: X, run: cancelEditing, disabled: saving.value },
        {
          label: 'Done',
          icon: Check,
          run: () => void finishEditing(),
          disabled: saving.value,
          primary: true,
        },
      ]
    : [
        {
          label: 'Edit layout',
          icon: LayoutGrid,
          run: startEditing,
          disabled: !dashboard.value || insights.isPending.value,
        },
      ],
)

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
      <DetailActions :actions="headerActions">
        <template v-if="!editing" #menu>
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
        :message="
          editing
            ? 'Add saved insights, then pick Done.'
            : 'Pick Edit layout to add saved insights.'
        "
      >
        <button v-if="editing" type="button" class="btn btn-sm" @click="openAdd">
          <Plus class="size-4" aria-hidden="true" />Add insight
        </button>
        <button v-else type="button" class="btn btn-sm" @click="startEditing">
          <LayoutGrid class="size-4" aria-hidden="true" />Edit layout
        </button>
      </EmptyState>
      <DashboardGrid v-else :items="gridItems" :editing="editing" :locked="saving" @move="moveTile">
        <template #tile="{ index, handle }">
          <InsightTile
            v-if="tiles[index]"
            :workspace-id="workspaceId"
            :insight="tiles[index].insight"
            :width="tiles[index].width"
            :range="range"
            :to="`${base}/insights/${tiles[index].insight.id}`"
            :editing="editing"
            :locked="saving"
            :handle="handle"
            @update:width="setWidth(index, $event)"
            @remove="removeTile(index)"
          />
        </template>
      </DashboardGrid>
    </template>

    <FormPanel
      :open="adding"
      title="Add insights"
      description="Pick saved insights of this project. They join the end of the dashboard at half width."
      submit-label="Add"
      :disabled="!picked.length || picked.length > room"
      @close="adding = false"
      @submit="onAdd"
    >
      <p v-if="!addable.length" class="text-sm text-base-content/70">
        Every saved insight of this project is on the dashboard.
      </p>
      <fieldset v-else class="flex flex-col">
        <legend class="mb-2 text-sm font-medium">Insights</legend>
        <div class="flex flex-col divide-y divide-base-300 rounded-box border border-base-300">
          <Checkbox
            v-for="insight in addable"
            :key="insight.id"
            v-model="picked"
            :value="insight.id"
            class="px-3 py-2.5"
          >
            <span class="flex min-w-0 flex-col">
              <span class="truncate">{{ insight.name }}</span>
              <span class="truncate text-xs text-base-content/70">{{
                describeQuery(insight.query)
              }}</span>
            </span>
          </Checkbox>
        </div>
      </fieldset>
      <p v-if="picked.length > room" class="text-sm text-error">
        A dashboard holds at most {{ MAX_TILES }} insights; pick {{ room }} or fewer.
      </p>
      <p class="text-sm text-base-content/70">
        Need another?
        <RouterLink class="link" :to="`${base}/insights/new?dashboard=${dashboardId}`"
          >New insight</RouterLink
        >
        opens the builder and brings you back here after you save it.
      </p>
    </FormPanel>

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
