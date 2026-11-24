<script setup lang="ts">
import { Mail } from 'lucide-vue-next'
import { computed } from 'vue'

import DataTable from '@/components/ui/DataTable.vue'
import PageSection from '@/components/ui/PageSection.vue'
import { useMailingLists } from '@/queries/mailing'

/**
 * The mailing lists a form adds its submitters to, on the form's page.
 * Renders nothing on other entity pages.
 */
const props = defineProps<{
  workspaceId: string
  projectId: string
  kind: string
  entityId: string
}>()

const isForm = computed(() => props.kind === 'forms.form')
const {
  data: lists,
  isPending,
  error,
} = useMailingLists(
  () => props.workspaceId,
  () => props.projectId,
  isForm,
)
const linked = computed(() =>
  (lists.value?.items ?? []).filter((list) => list.form_ids.includes(props.entityId)),
)
const columns = [
  { key: 'name', label: 'List' },
  { key: 'description', label: 'Description' },
]
const mailingPath = computed(() => `/w/${props.workspaceId}/p/${props.projectId}/mailing`)
</script>

<template>
  <PageSection v-if="isForm" title="Mailing lists">
    <template #description>
      Submitters with an email address join these lists after they confirm by mail. Link a form in a
      list’s settings.
    </template>
    <DataTable
      :columns="columns"
      :rows="isPending ? undefined : linked"
      :row-key="(l) => l.id"
      :row-to="(r) => `${mailingPath}/lists/${r.id}`"
      :loading="isPending"
      :error="error"
      label="Mailing lists"
      :empty="{
        icon: Mail,
        title: 'No lists',
        message: 'This form adds nobody to a mailing list.',
      }"
    >
      <template #cell-name="{ row }">
        {{ row.name }}
      </template>
      <template #cell-description="{ row }"
        ><span class="text-sm text-base-content/80">{{ row.description ?? '' }}</span></template
      >
      <template #empty-action>
        <RouterLink :to="mailingPath" class="btn btn-sm">Open Mailing</RouterLink>
      </template>
    </DataTable>
  </PageSection>
</template>
