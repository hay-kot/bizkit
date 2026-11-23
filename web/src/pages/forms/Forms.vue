<script setup lang="ts">
import { Inbox, Plus } from 'lucide-vue-next'
import { ref } from 'vue'
import { useRouter } from 'vue-router'

import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import { formatDate } from '@/lib/format'
import { useCreateForm, useForms } from '@/queries/forms'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const router = useRouter()
const {
  data: forms,
  isPending,
  error,
} = useForms(
  () => props.workspaceId,
  () => props.projectId,
)
const create = useCreateForm()
const panelOpen = ref(false)
const name = ref('')
const columns = [
  { key: 'name', label: 'Name' },
  { key: 'inbox', label: 'Inbox' },
  { key: 'check', label: 'Bot check' },
  { key: 'created', label: 'Created' },
]

function openPanel() {
  name.value = ''
  create.reset()
  panelOpen.value = true
}

function formPath(formId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/forms/${formId}`
}

async function onCreate() {
  const form = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { name: name.value, project_id: props.projectId },
  })
  panelOpen.value = false
  await router.push(formPath(form.id))
}
</script>

<template>
  <Page>
    <PageHeader
      :section="projectName"
      title="Forms"
      description="Collect submissions from your sites, with spam kept out."
    >
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New form
      </button>
    </PageHeader>

    <DataTable
      :columns="columns"
      :rows="forms?.items"
      :row-key="(f) => f.id"
      :loading="isPending"
      :error="error"
      :empty="{
        icon: Inbox,
        title: 'No forms yet',
        message: 'Create a form, then point your site’s form at its endpoint.',
      }"
    >
      <template #cell-name="{ row }">
        <RouterLink :to="formPath(row.id)" class="link font-medium link-hover">
          {{ row.name }}
        </RouterLink>
        <span v-if="!row.enabled" class="ms-2 badge badge-ghost badge-sm">Disabled</span>
      </template>
      <template #cell-inbox="{ row }"
        ><span class="text-sm tabular-nums">{{ row.inbox_count }}</span></template
      >
      <template #cell-check="{ row }"
        ><span class="text-sm">{{
          row.bot_check === 'challenge' ? 'Challenge' : 'Honeypot'
        }}</span></template
      >
      <template #cell-created="{ row }"
        ><span class="text-sm">{{ formatDate(row.created_at) }}</span></template
      >
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">New form</button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      title="New form"
      submit-label="Create form"
      :pending="create.asyncStatus.value === 'loading'"
      :error="create.error.value"
      @close="panelOpen = false"
      @submit="onCreate"
    >
      <Field label="Name">
        <input
          v-model="name"
          name="form-name"
          required
          class="input w-full"
          placeholder="Contact"
        />
      </Field>
    </FormPanel>
  </Page>
</template>
