<script setup lang="ts">
import { ListOrdered, Plus } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import TabPage from '@/components/ui/TabPage.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Select from '@/components/ui/Select.vue'
import { useCreateIntent } from '@/composables/useCreateIntent'
import { formatNumber } from '@/lib/format'
import {
  type SequenceTrigger,
  useCreateSequence,
  useMailingLists,
  useSequences,
} from '@/queries/mailing'

const props = defineProps<{ workspaceId: string; projectId: string }>()
const router = useRouter()
const {
  data: sequences,
  isPending,
  error,
} = useSequences(
  () => props.workspaceId,
  () => props.projectId,
)
const { data: lists } = useMailingLists(
  () => props.workspaceId,
  () => props.projectId,
)
const listOptions = computed(() =>
  (lists.value?.items ?? []).map((list) => ({ value: list.id, label: list.name })),
)
const listName = (id: string) => lists.value?.items.find((l) => l.id === id)?.name ?? ''
const triggerOptions: { value: SequenceTrigger; label: string }[] = [
  { value: 'on_join', label: 'When someone subscribes' },
  { value: 'manual', label: 'Only when enrolled by hand or the API' },
]
const triggerLabel = (trigger: SequenceTrigger) =>
  trigger === 'on_join' ? 'On subscribe' : 'Manual'

const create = useCreateSequence()
const panelOpen = ref(false)
useCreateIntent(openPanel)
const listId = ref('')
const name = ref('')
const trigger = ref<SequenceTrigger>('on_join')
const columns = [
  { key: 'name', label: 'Name' },
  { key: 'list', label: 'List' },
  { key: 'trigger', label: 'Starts' },
  { key: 'steps', label: 'Steps' },
  { key: 'enabled', label: 'Enrolling' },
]

// `?new=1` opens the panel on mount, often before the lists load.
watch(
  () => lists.value?.items[0]?.id,
  (first) => {
    if (panelOpen.value && listId.value === '' && first) listId.value = first
  },
)

function openPanel() {
  listId.value = lists.value?.items[0]?.id ?? ''
  name.value = ''
  trigger.value = 'on_join'
  create.reset()
  panelOpen.value = true
}

function sequencePath(sequenceId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/mailing/sequences/${sequenceId}`
}

async function onCreate() {
  const sequence = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: {
      list_id: listId.value,
      name: name.value,
      trigger: trigger.value,
      // Disabled until the steps are written, so nobody enrolls in an
      // empty sequence and completes it at once.
      enabled: false,
      steps: [],
    },
  })
  panelOpen.value = false
  await router.push(sequencePath(sequence.id))
}
</script>

<template>
  <TabPage title="Mailing" description="Lists people subscribe to, and the mail you send them.">
    <template #actions>
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New sequence
      </button>
    </template>

    <DataTable
      :columns="columns"
      :rows="sequences?.items"
      :row-key="(s) => s.id"
      :row-to="(r) => sequencePath(r.id)"
      :loading="isPending"
      :error="error"
      label="Sequences"
      :empty="{
        icon: ListOrdered,
        title: 'No sequences yet',
        message: 'A sequence sends timed mails, such as a welcome series for new subscribers.',
      }"
    >
      <template #cell-name="{ row }">
        {{ row.name }}
      </template>
      <template #cell-list="{ row }"
        ><span class="text-sm">{{ listName(row.list_id) }}</span></template
      >
      <template #cell-trigger="{ row }"
        ><span class="text-sm">{{ triggerLabel(row.trigger) }}</span></template
      >
      <template #cell-steps="{ row }"
        ><span class="text-sm tabular-nums">{{ formatNumber(row.steps.length) }}</span></template
      >
      <template #cell-enabled="{ row }"
        ><span class="badge badge-sm" :class="row.enabled ? 'badge-success' : 'badge-ghost'">{{
          row.enabled ? 'on' : 'off'
        }}</span></template
      >
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">
          New sequence
        </button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      title="New sequence"
      description="It starts switched off. Write the steps, then turn on enrolling."
      submit-label="Create sequence"
      :pending="create.asyncStatus.value === 'loading'"
      :error="create.error.value"
      :disabled="listId === ''"
      @close="panelOpen = false"
      @submit="onCreate"
    >
      <p v-if="listOptions.length === 0" class="text-sm text-base-content/80">
        Create a list first.
      </p>
      <Field v-else label="List">
        <Select v-model="listId" :options="listOptions" label="List" name="sequence-list" />
      </Field>
      <Field label="Name">
        <input
          v-model="name"
          name="sequence-name"
          required
          class="input w-full"
          placeholder="Welcome series"
        />
      </Field>
      <Field label="Starts">
        <Select
          v-model="trigger"
          :options="triggerOptions"
          label="Starts"
          name="sequence-trigger"
        />
      </Field>
    </FormPanel>
  </TabPage>
</template>
