<script setup lang="ts">
import { LayoutDashboard, Pencil, Trash2 } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import InsightBuilder from '@/components/analytics/InsightBuilder.vue'
import {
  draftOf,
  emptyInsight,
  type InsightDraft,
  isRunnable,
  queryOf,
} from '@/components/analytics/insightDraft'
import ErrorAlert from '@/components/ErrorAlert.vue'
import DetailActions from '@/components/ui/DetailActions.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import Select from '@/components/ui/Select.vue'
import { useReportRange } from '@/composables/useReportRange'
import { runAction } from '@/lib/action'
import { type DateRange, presetRange, savedPreset } from '@/lib/range'
import {
  type InsightRange,
  useCreateDashboard,
  useDashboards,
  useDeleteInsight,
  useInsight,
  useUpdateDashboard,
  useUpdateInsight,
} from '@/queries/analytics'

const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  insightId: string
}>()
const route = useRoute()
const router = useRouter()
const { today, range: urlRange } = useReportRange()
const { data: insight, error: insightError } = useInsight(
  () => props.workspaceId,
  () => props.insightId,
)
const base = computed(() => `/w/${props.workspaceId}/p/${props.projectId}/analytics`)

// The URL's project comes from the link, the insight's from the server.
watch(insight, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(
      `/w/${props.workspaceId}/p/${loaded.project_id}/analytics/insights/${loaded.id}`,
    )
  }
})

/** The URL's range when it names one, else the insight's own. */
const range = computed<DateRange>({
  get: () => {
    const saved = insight.value?.range
    if (route.query.range || !saved) return urlRange.value
    return { preset: saved, ...presetRange(saved, today.value) }
  },
  set: (next) => (urlRange.value = next),
})

const draft = ref<InsightDraft>(emptyInsight())
// Seed the draft once per insight: refetches after a rename or a pin must
// not throw away edits that aren't saved yet.
watch(
  () => insight.value?.id,
  (id) => {
    if (id && insight.value) draft.value = draftOf(insight.value.query)
  },
  { immediate: true },
)
const savedRange = computed((): InsightRange => {
  return savedPreset(range.value.preset) ?? insight.value?.range ?? '30d'
})
const dirty = computed(() => {
  const saved = insight.value
  if (!saved) return false
  return (
    JSON.stringify(queryOf(draft.value)) !== JSON.stringify(queryOf(draftOf(saved.query))) ||
    savedRange.value !== saved.range
  )
})

const update = useUpdateInsight()
function onSave() {
  return runAction({
    run: () =>
      update.mutateAsync({
        path: { workspace_id: props.workspaceId, insight_id: props.insightId },
        body: { query: queryOf(draft.value), range: savedRange.value },
      }),
    success: 'Insight saved',
  })
}

const renaming = ref(false)
const name = ref('')
function openRename() {
  name.value = insight.value?.name ?? ''
  update.reset()
  renaming.value = true
}
async function onRename() {
  await update.mutateAsync({
    path: { workspace_id: props.workspaceId, insight_id: props.insightId },
    body: { name: name.value },
  })
  renaming.value = false
}

const NEW_DASHBOARD = '__new__'
const dashboards = useDashboards(
  () => props.workspaceId,
  () => props.projectId,
)
const pinning = ref(false)
const target = ref(NEW_DASHBOARD)
const dashboardName = ref('')
const dashboardOptions = computed(() => [
  ...(dashboards.data.value?.items ?? [])
    .filter((board) => !board.tiles.some((tile) => tile.insight_id === props.insightId))
    .map((board) => ({ value: board.id, label: board.name })),
  { value: NEW_DASHBOARD, label: 'New dashboard' },
])
const createDashboard = useCreateDashboard()
const updateDashboard = useUpdateDashboard()
const pinPending = computed(
  () => createDashboard.isLoading.value || updateDashboard.isLoading.value,
)
const pinError = computed(() => createDashboard.error.value ?? updateDashboard.error.value)
function openPin() {
  target.value = dashboardOptions.value[0]?.value ?? NEW_DASHBOARD
  dashboardName.value = ''
  createDashboard.reset()
  updateDashboard.reset()
  pinning.value = true
}
async function onPin() {
  const path = { workspace_id: props.workspaceId }
  let boardId = target.value
  if (boardId === NEW_DASHBOARD) {
    const board = await createDashboard.mutateAsync({
      path,
      body: {
        project_id: props.projectId,
        name: dashboardName.value,
        tiles: [{ insight_id: props.insightId, width: 6 }],
      },
    })
    boardId = board.id
  } else {
    const board = dashboards.data.value?.items.find((b) => b.id === boardId)
    await updateDashboard.mutateAsync({
      path: { ...path, dashboard_id: boardId },
      body: { tiles: [...(board?.tiles ?? []), { insight_id: props.insightId, width: 6 }] },
    })
  }
  pinning.value = false
  await router.push(`${base.value}/dashboards/${boardId}`)
}

const remove = useDeleteInsight()
function onDelete() {
  const saved = insight.value
  const label = saved?.name ?? 'this insight'
  const boards = saved?.dashboards.map((d) => d.name) ?? []
  return runAction({
    confirm: {
      title: `Delete ${label}?`,
      message: boards.length
        ? `It also leaves ${boards.length === 1 ? 'the dashboard' : 'the dashboards'} ${boards.join(', ')}.`
        : 'This removes the saved insight. Events stay.',
      confirmLabel: 'Delete insight',
    },
    run: async () => {
      await remove.mutateAsync({
        path: { workspace_id: props.workspaceId, insight_id: props.insightId },
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
    :title="insight?.name ?? 'Insight'"
    :description="
      insight?.dashboards.length
        ? `On ${insight.dashboards.map((d) => d.name).join(', ')}.`
        : 'A saved insight. Change it and save, or add it to a dashboard.'
    "
  >
    <template #actions>
      <DetailActions
        :actions="[
          { label: 'Add to dashboard', icon: LayoutDashboard, run: openPin, disabled: !insight },
        ]"
        :save="{
          form: 'insight-form',
          dirty,
          pending: update.isLoading.value && !renaming,
          blocked: isRunnable(draft) ? undefined : 'Pick an event first.',
        }"
      >
        <template #menu>
          <MenuItem @select="openRename"
            ><Pencil class="size-4" aria-hidden="true" />Rename</MenuItem
          >
          <MenuItem danger @select="onDelete">
            <Trash2 class="size-4" aria-hidden="true" />Delete insight
          </MenuItem>
        </template>
      </DetailActions>
    </template>
    <ErrorAlert v-if="insightError" :error="insightError" />
    <p v-if="!savedPreset(range.preset)" class="text-sm text-base-content/70">
      This range isn't saved. The insight opens on its saved range.
    </p>
    <form id="insight-form" class="contents" @submit.prevent="onSave">
      <InsightBuilder
        v-if="insight"
        v-model:range="range"
        v-model:draft="draft"
        :workspace-id="workspaceId"
        :project-id="projectId"
        :today="today"
      />
    </form>

    <FormPanel
      :open="renaming"
      variant="dialog"
      title="Rename insight"
      submit-label="Rename"
      :pending="update.isLoading.value"
      :error="update.error.value"
      @close="renaming = false"
      @submit="onRename"
    >
      <Field label="Name">
        <input v-model="name" name="insight-name" class="input w-full" required maxlength="100" />
      </Field>
    </FormPanel>

    <FormPanel
      :open="pinning"
      variant="dialog"
      title="Add to dashboard"
      submit-label="Add to dashboard"
      :pending="pinPending"
      :error="pinError"
      @close="pinning = false"
      @submit="onPin"
    >
      <Field label="Dashboard">
        <Select v-model="target" :options="dashboardOptions" label="Dashboard" name="dashboard" />
      </Field>
      <Field v-if="target === NEW_DASHBOARD" label="New dashboard name">
        <input
          v-model="dashboardName"
          name="dashboard-name"
          class="input w-full"
          required
          maxlength="100"
        />
      </Field>
    </FormPanel>
  </AnalyticsShell>
</template>
