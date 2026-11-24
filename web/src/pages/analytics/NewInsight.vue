<script setup lang="ts">
import { Save } from 'lucide-vue-next'
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import AnalyticsShell from '@/components/analytics/AnalyticsShell.vue'
import InsightBuilder from '@/components/analytics/InsightBuilder.vue'
import { draftFromLink, isRunnable, queryOf } from '@/components/analytics/insightDraft'
import DetailActions from '@/components/ui/DetailActions.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import { useReportRange } from '@/composables/useReportRange'
import { rangeToQuery } from '@/lib/range'
import { type InsightRange, useCreateInsight } from '@/queries/analytics'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const route = useRoute()
const router = useRouter()
const { today, range } = useReportRange()
const draft = ref(draftFromLink(route.query))
const create = useCreateInsight()
const naming = ref(false)
const name = ref('')

function openSave() {
  name.value = draft.value.kind === 'people' ? 'People' : draft.value.event
  create.reset()
  naming.value = true
}

async function onSave() {
  const preset = range.value.preset
  const insight = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: {
      project_id: props.projectId,
      name: name.value,
      query: queryOf(draft.value),
      // A custom range would go stale, so the insight opens on 30 days.
      range: (preset === 'custom' ? '30d' : preset) as InsightRange,
    },
  })
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
          { label: 'Save insight', icon: Save, run: openSave, disabled: !isRunnable(draft) },
        ]"
      />
    </template>
    <InsightBuilder
      v-model:range="range"
      v-model:draft="draft"
      :workspace-id="workspaceId"
      :project-id="projectId"
      :today="today"
    />
    <FormPanel
      :open="naming"
      variant="dialog"
      title="Save insight"
      submit-label="Save insight"
      :pending="create.isLoading.value"
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
