<script setup lang="ts">
import { Plus, Send } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import StatusBadge from '@/components/mailing/StatusBadge.vue'
import TabPage from '@/components/ui/TabPage.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Select from '@/components/ui/Select.vue'
import { useCreateIntent } from '@/composables/useCreateIntent'
import { formatDateTime, formatNumber } from '@/lib/format'
import { useBroadcasts, useCreateBroadcast, useMailingLists } from '@/queries/mailing'

const props = defineProps<{ workspaceId: string; projectId: string }>()
const router = useRouter()
const {
  data: broadcasts,
  isPending,
  error,
} = useBroadcasts(
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
const create = useCreateBroadcast()
const panelOpen = ref(false)
useCreateIntent(openPanel)
const listId = ref('')
const subject = ref('')
const columns = [
  { key: 'subject', label: 'Subject' },
  { key: 'list', label: 'List' },
  { key: 'status', label: 'Status' },
  { key: 'recipients', label: 'Recipients' },
  { key: 'sent', label: 'Sent' },
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
  subject.value = ''
  create.reset()
  panelOpen.value = true
}

function broadcastPath(broadcastId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/mailing/broadcasts/${broadcastId}`
}

async function onCreate() {
  const broadcast = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { list_id: listId.value, subject: subject.value, body_md: '' },
  })
  panelOpen.value = false
  await router.push(broadcastPath(broadcast.id))
}
</script>

<template>
  <TabPage title="Mailing" description="Lists people subscribe to, and the mail you send them.">
    <template #actions>
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New broadcast
      </button>
    </template>

    <DataTable
      :columns="columns"
      :rows="broadcasts?.items"
      :row-key="(b) => b.id"
      :row-to="(r) => broadcastPath(r.id)"
      :loading="isPending"
      :error="error"
      label="Broadcasts"
      :empty="{
        icon: Send,
        title: 'No broadcasts yet',
        message: 'Write a broadcast, test it on yourself, then send it to a list.',
      }"
    >
      <template #cell-subject="{ row }">
        {{ row.subject }}
        <span v-if="row.archived" class="ms-2 badge badge-ghost badge-sm">archive</span>
      </template>
      <template #cell-list="{ row }"
        ><span class="text-sm">{{ listName(row.list_id) }}</span></template
      >
      <template #cell-status="{ row }"><StatusBadge :status="row.status" /></template>
      <template #cell-recipients="{ row }"
        ><span class="text-sm tabular-nums">{{
          row.status === 'draft' || row.status === 'scheduled' ? '—' : formatNumber(row.recipients)
        }}</span></template
      >
      <template #cell-sent="{ row }"
        ><span class="text-sm whitespace-nowrap">{{
          row.status === 'scheduled'
            ? `Scheduled for ${formatDateTime(row.send_at)}`
            : formatDateTime(row.sent_at)
        }}</span></template
      >
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">
          New broadcast
        </button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      title="New broadcast"
      submit-label="Create draft"
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
        <Select v-model="listId" :options="listOptions" label="List" name="broadcast-list" />
      </Field>
      <Field label="Subject" hint="{{ name }} fills in each subscriber’s name.">
        <input v-model="subject" name="broadcast-subject" required class="input w-full" />
      </Field>
    </FormPanel>
  </TabPage>
</template>
