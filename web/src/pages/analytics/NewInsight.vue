<script setup lang="ts">
import { Save } from 'lucide-vue-next'
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import { currentQuery, editorOfDraft, saveBlocker } from '@/components/analytics/insightCode'
import { draftFromLink } from '@/components/analytics/insightDraft'
import InsightEditor from '@/components/analytics/InsightEditor.vue'
import DetailActions from '@/components/ui/DetailActions.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import { useReportRange } from '@/composables/useReportRange'
import { runAction } from '@/lib/action'
import { rangeToQuery, savedPreset } from '@/lib/range'
import { toast } from '@/lib/toast'
import { useCreateInsight, useDashboard, useUpdateDashboard } from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const route = useRoute()
const router = useRouter()
const { today, range } = useReportRange()
const editor = ref(editorOfDraft(draftFromLink(route.query)))
const create = useCreateInsight()
// Opened from a dashboard's Add insight panel: the saved insight joins that
// dashboard and the user goes back to it.
const dashboardId = typeof route.query.dashboard === 'string' ? route.query.dashboard : undefined
const target = dashboardId ? useDashboard(() => props.workspaceId, dashboardId) : undefined
const updateDashboard = useUpdateDashboard()
const naming = ref(false)
const name = ref('')

function openSave() {
  const query = currentQuery(editor.value)
  if (!query.ok) return
  name.value = query.value.kind === 'people' ? 'People' : (query.value.event ?? '')
  create.reset()
  naming.value = true
}

async function onSave() {
  const query = currentQuery(editor.value)
  if (!query.ok) return
  const insight = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: {
      project_id: props.projectId,
      name: name.value,
      query: query.value,
      // A range the API can't keep (custom dates would go stale), so the
      // insight opens on 30 days.
      range: savedPreset(range.value.preset) ?? '30d',
    },
  })
  // Waits for the dashboard when it hasn't loaded yet. The insight is saved
  // either way; when the dashboard is gone or refuses it (full), a toast
  // says why and the user lands on the insight instead.
  const board = target ? (await target.refresh()).data : undefined
  if (target && !board) toast('Saved the insight, but its dashboard was not found', 'error')
  const added =
    board &&
    (await runAction({
      run: () =>
        updateDashboard.mutateAsync({
          path: { workspace_id: props.workspaceId, dashboard_id: board.id },
          body: { tiles: [...board.tiles, { insight_id: insight.id, width: 6 }] },
        }),
      success: `Added to ${board.name}`,
    }))
  if (board && added) {
    naming.value = false
    await router.push({
      path: `/w/${props.workspaceId}/p/${props.projectId}/analytics/dashboards/${board.id}`,
      query: rangeToQuery(range.value),
    })
    return
  }
  naming.value = false
  await router.push({
    path: `/w/${props.workspaceId}/p/${props.projectId}/analytics/insights/${insight.id}`,
    query: rangeToQuery(range.value),
  })
}
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    title="New insight"
    description="Count events or people, filter them, and split them by a property or trait."
  >
    <template #actions>
      <DetailActions
        :actions="[
          {
            label: 'Save insight',
            icon: Save,
            run: openSave,
            disabled: saveBlocker(editor) !== undefined,
          },
        ]"
      />
    </template>
    <p v-if="target?.data.value" class="text-sm text-base-content/70">
      Saving adds it to {{ target.data.value.name }} and takes you back there.
    </p>
    <p v-else-if="target?.error.value" class="text-sm text-warning">
      The dashboard this came from was not found. Saving keeps the insight on its own.
    </p>
    <p v-if="!savedPreset(range.preset)" class="text-sm text-base-content/70">
      This range isn't saved. A saved insight opens on the last 30 days.
    </p>
    <InsightEditor
      v-model:range="range"
      v-model:editor="editor"
      :workspace-id="workspaceId"
      :project-id="projectId"
      :today="today"
    />
    <FormPanel
      :open="naming"
      variant="dialog"
      title="Save insight"
      submit-label="Save insight"
      :pending="create.isLoading.value || updateDashboard.isLoading.value"
      :error="create.error.value"
      @close="naming = false"
      @submit="onSave"
    >
      <Field label="Name">
        <input v-model="name" name="insight-name" class="input w-full" required maxlength="100" />
      </Field>
    </FormPanel>
  </AnalyticsShell>
</template>
