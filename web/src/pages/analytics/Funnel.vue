<script setup lang="ts">
import { Pencil, Trash2 } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import FunnelForm from '@/components/analytics/FunnelForm.vue'
import FunnelSteps from '@/components/analytics/FunnelSteps.vue'
import {
  draftOf,
  emptyFunnel,
  type FunnelDraft,
  funnelBody,
} from '@/components/analytics/funnelDraft'
import RangeToolbar from '@/components/analytics/RangeToolbar.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import DetailActions from '@/components/ui/DetailActions.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { useReportRange } from '@/composables/useReportRange'
import { runAction } from '@/lib/action'
import { useDeleteFunnel, useFunnel, useFunnelResults, useUpdateFunnel } from '@/queries/analytics'

const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  funnelId: string
}>()
const router = useRouter()
const { today, range } = useReportRange()
const { data: funnel, error: funnelError } = useFunnel(
  () => props.workspaceId,
  () => props.funnelId,
)
// The URL's project comes from the link, the funnel's from the server.
watch(funnel, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(
      `/w/${props.workspaceId}/p/${loaded.project_id}/analytics/funnels/${loaded.id}`,
    )
  }
})
const results = useFunnelResults(
  () => props.workspaceId,
  () => props.funnelId,
  range,
)
const update = useUpdateFunnel()
const remove = useDeleteFunnel()
const listPath = computed(() => `/w/${props.workspaceId}/p/${props.projectId}/analytics/funnels`)

const editing = ref(false)
const draft = ref<FunnelDraft>(emptyFunnel())
function openEdit() {
  if (!funnel.value) return
  draft.value = draftOf(funnel.value)
  update.reset()
  editing.value = true
}

async function onSave() {
  await update.mutateAsync({
    path: { workspace_id: props.workspaceId, funnel_id: props.funnelId },
    body: funnelBody(draft.value),
  })
  editing.value = false
}

function onDelete() {
  const name = funnel.value?.name ?? 'this funnel'
  return runAction({
    confirm: {
      title: `Delete ${name}?`,
      message: 'This removes the funnel. Events stay.',
      confirmLabel: 'Delete funnel',
    },
    run: async () => {
      await remove.mutateAsync({
        path: { workspace_id: props.workspaceId, funnel_id: props.funnelId },
      })
      return true
    },
    success: `Deleted ${name}`,
  }).then(async (deleted) => {
    if (deleted) await router.push(listPath.value)
  })
}
</script>

<template>
  <AnalyticsShell
    :workspace-id="workspaceId"
    :project-id="projectId"
    :project-name="projectName"
    gate="events"
    :title="funnel?.name ?? 'Funnel'"
    :description="
      funnel
        ? `Each step must follow the one before, within ${funnel.window_days} days of the first.`
        : undefined
    "
  >
    <template #actions>
      <DetailActions :actions="[{ label: 'Edit', icon: Pencil, run: openEdit, disabled: !funnel }]">
        <template #menu>
          <MenuItem danger @select="onDelete">
            <Trash2 class="size-4" aria-hidden="true" />Delete funnel
          </MenuItem>
        </template>
      </DetailActions>
    </template>
    <template #default="{ site }">
      <ErrorAlert v-if="funnelError" :error="funnelError" />
      <RangeToolbar v-model:range="range" :today="today" />
      <p v-if="site.identity_mode === 'cookieless'" class="text-sm text-base-content/70">
        This project is cookieless, so a visitor's steps only connect within one UTC day.
      </p>
      <ErrorAlert v-if="results.error.value" :error="results.error.value" />
      <SkeletonRows v-else-if="results.isPending.value" :rows="4" />
      <FunnelSteps v-else-if="results.data.value" :steps="results.data.value.steps" />

      <FormPanel
        :open="editing"
        title="Edit funnel"
        submit-label="Save funnel"
        :pending="update.asyncStatus.value === 'loading'"
        :error="update.error.value"
        @close="editing = false"
        @submit="onSave"
      >
        <FunnelForm v-model="draft" />
      </FormPanel>
    </template>
  </AnalyticsShell>
</template>
