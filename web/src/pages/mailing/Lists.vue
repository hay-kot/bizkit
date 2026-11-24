<script setup lang="ts">
import { Mail, Plus } from 'lucide-vue-next'
import { ref } from 'vue'
import { useRouter } from 'vue-router'

import Checkbox from '@/components/ui/Checkbox.vue'
import TabPage from '@/components/ui/TabPage.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import { useCreateIntent } from '@/composables/useCreateIntent'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { formatDate } from '@/lib/format'
import { useCreateMailingList, useMailingLists } from '@/queries/mailing'

const props = defineProps<{ workspaceId: string; projectId: string }>()
const router = useRouter()
const isAdmin = useIsAdmin(() => props.workspaceId)
const {
  data: lists,
  isPending,
  error,
} = useMailingLists(
  () => props.workspaceId,
  () => props.projectId,
)
const create = useCreateMailingList()
const panelOpen = ref(false)
useCreateIntent(openPanel)
const name = ref('')
const description = ref('')
const doubleOptIn = ref(true)
const columns = [
  { key: 'name', label: 'Name' },
  { key: 'consent', label: 'Double opt-in' },
  { key: 'forms', label: 'Forms' },
  { key: 'created', label: 'Created' },
]

function openPanel() {
  if (!isAdmin.value) return
  name.value = ''
  description.value = ''
  doubleOptIn.value = true
  create.reset()
  panelOpen.value = true
}

function listPath(listId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/mailing/lists/${listId}`
}

async function onCreate() {
  const text = description.value.trim()
  const list = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: {
      name: name.value,
      project_id: props.projectId,
      description: text === '' ? null : text,
      double_opt_in: doubleOptIn.value,
    },
  })
  panelOpen.value = false
  await router.push(listPath(list.id))
}
</script>

<template>
  <TabPage title="Mailing" description="Lists people subscribe to, and the mail you send them.">
    <template #actions>
      <button
        v-if="isAdmin"
        type="button"
        class="btn gap-1.5 btn-primary btn-sm"
        @click="openPanel"
      >
        <Plus class="size-4" aria-hidden="true" />New list
      </button>
    </template>

    <DataTable
      :columns="columns"
      :rows="lists?.items"
      :row-key="(l) => l.id"
      :loading="isPending"
      :error="error"
      label="Lists"
      :empty="{
        icon: Mail,
        title: 'No lists yet',
        message: 'Create a list, then link a form or add addresses to it.',
      }"
    >
      <template #cell-name="{ row }">
        <RouterLink :to="listPath(row.id)" class="link font-medium link-hover">
          {{ row.name }}
        </RouterLink>
      </template>
      <template #cell-consent="{ row }"
        ><span class="text-sm">{{ row.double_opt_in ? 'On' : 'Off' }}</span></template
      >
      <template #cell-forms="{ row }"
        ><span class="text-sm tabular-nums">{{ row.form_ids.length }}</span></template
      >
      <template #cell-created="{ row }"
        ><span class="text-sm">{{ formatDate(row.created_at) }}</span></template
      >
      <template v-if="isAdmin" #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">New list</button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      title="New list"
      submit-label="Create list"
      :pending="create.asyncStatus.value === 'loading'"
      :error="create.error.value"
      @close="panelOpen = false"
      @submit="onCreate"
    >
      <Field label="Name" hint="Shown at the top of every mail.">
        <input
          v-model="name"
          name="list-name"
          required
          class="input w-full"
          placeholder="Newsletter"
        />
      </Field>
      <Field label="Description" hint="Optional. What subscribers get, for your team.">
        <input v-model="description" name="list-description" class="input w-full" />
      </Field>
      <Checkbox v-model="doubleOptIn" label="Addresses added by hand or the API confirm by mail" />
      <p class="text-sm text-base-content/70">
        Addresses from forms always confirm, whatever this says.
      </p>
    </FormPanel>
  </TabPage>
</template>
