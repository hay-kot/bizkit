<script setup lang="ts">
import { refDebounced } from '@vueuse/core'
import { Ban, Download, Eraser, Plus, Trash2 } from 'lucide-vue-next'
import { computed, ref } from 'vue'

import MailingTabs from '@/components/mailing/MailingTabs.vue'
import StatusBadge from '@/components/mailing/StatusBadge.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'
import Select from '@/components/ui/Select.vue'
import { useCreateIntent } from '@/composables/useCreateIntent'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { runAction } from '@/lib/action'
import { formatDate } from '@/lib/format'
import {
  downloadAddressExport,
  ROWS_SHOWN,
  type Suppression,
  type SuppressionReason,
  useAddSuppression,
  useEraseAddress,
  useRemoveSuppression,
  useSuppressions,
} from '@/queries/mailing'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const isAdmin = useIsAdmin(() => props.workspaceId)

const search = ref('')
const query = refDebounced(search, 250)
const reason = ref<'' | SuppressionReason>('')
const reasonOptions: { value: '' | SuppressionReason; label: string }[] = [
  { value: '', label: 'Any reason' },
  { value: 'bounce', label: 'Bounced' },
  { value: 'complaint', label: 'Complained' },
  { value: 'manual', label: 'Added by hand' },
]
const {
  data: suppressions,
  isPending,
  error,
} = useSuppressions(
  () => props.workspaceId,
  () => ({ reason: reason.value === '' ? undefined : reason.value, q: query.value }),
)

const columns = [
  { key: 'email', label: 'Address' },
  { key: 'reason', label: 'Reason' },
  { key: 'note', label: 'Note' },
  { key: 'added', label: 'Added' },
  { key: 'actions', label: '' },
]

const add = useAddSuppression()
const remove = useRemoveSuppression()
const panelOpen = ref(false)
const email = ref('')
const note = ref('')
useCreateIntent(openPanel)

function openPanel() {
  email.value = ''
  note.value = ''
  add.reset()
  panelOpen.value = true
}

async function onAdd() {
  const trimmed = note.value.trim()
  await add.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { email: email.value, note: trimmed === '' ? null : trimmed },
  })
  panelOpen.value = false
}

const erase = useEraseAddress()
const addressMode = ref<'export' | 'erase' | null>(null)
const address = ref('')
const exporting = ref(false)
const addressPending = computed(() => exporting.value || erase.asyncStatus.value === 'loading')

function openAddress(mode: 'export' | 'erase') {
  address.value = ''
  erase.reset()
  addressMode.value = mode
}

async function onAddress() {
  const email = address.value.trim()
  if (addressMode.value === 'export') {
    exporting.value = true
    const done = await runAction({
      run: () => downloadAddressExport(props.workspaceId, email),
      success: (data) => `Exported ${data.email}`,
    }).finally(() => (exporting.value = false))
    if (done) addressMode.value = null
    return
  }
  // The confirm opens beneath an open dialog, so the form closes first.
  addressMode.value = null
  await runAction({
    confirm: {
      title: `Erase ${email}?`,
      message:
        "This deletes the address's contacts, sequence runs and send log in every project of " +
        'this workspace, and keeps only a hash so it is never mailed or added again. ' +
        "This can't be undone.",
      confirmLabel: 'Erase',
      danger: true,
    },
    run: () => erase.mutateAsync({ path: { workspace_id: props.workspaceId }, body: { email } }),
    success: (r) => `Erased ${email}: ${r.contacts} contacts, ${r.deliveries} mails`,
  })
}

function onRemove(row: Suppression) {
  return runAction({
    confirm: {
      title: `Remove ${row.email}?`,
      message:
        'Lists where the address is still subscribed mail it again. Nobody is subscribed again.',
      confirmLabel: 'Remove',
    },
    run: () => remove.mutateAsync({ path: { workspace_id: props.workspaceId, email: row.email } }),
    success: `Removed ${row.email}`,
  })
}
</script>

<template>
  <Page>
    <PageHeader
      :section="projectName"
      title="Mailing"
      description="Lists people subscribe to, and the mail you send them."
    >
      <div v-if="isAdmin" class="flex flex-wrap gap-2">
        <button type="button" class="btn gap-1.5 btn-ghost btn-sm" @click="openAddress('export')">
          <Download class="size-4" aria-hidden="true" />Export address
        </button>
        <button type="button" class="btn gap-1.5 btn-ghost btn-sm" @click="openAddress('erase')">
          <Eraser class="size-4" aria-hidden="true" />Erase address
        </button>
        <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
          <Plus class="size-4" aria-hidden="true" />Suppress address
        </button>
      </div>
    </PageHeader>

    <MailingTabs :workspace-id="workspaceId" :project-id="projectId" />

    <PageSection title="Suppressions">
      <template #description>
        Addresses that get no mail from any list in this workspace, in every project. Bounces and
        spam complaints land here on their own. A complaint stays for good.
      </template>
      <template #actions>
        <input
          v-model="search"
          type="search"
          name="suppression-search"
          aria-label="Search suppressions"
          placeholder="Search"
          class="input w-36 input-sm sm:w-48"
        />
        <div class="w-36 sm:w-40">
          <Select v-model="reason" :options="reasonOptions" label="Reason" size="sm" />
        </div>
      </template>
      <DataTable
        :columns="columns"
        :rows="suppressions?.items"
        :row-key="(s) => s.email"
        :loading="isPending"
        :error="error"
        label="Suppressions"
        :empty="{
          icon: Ban,
          title: 'No suppressed addresses',
          message: 'Addresses that bounce or report spam show up here.',
        }"
      >
        <template #cell-email="{ row }"
          ><span class="text-sm font-medium">{{ row.email }}</span></template
        >
        <template #cell-reason="{ row }"><StatusBadge :status="row.reason" /></template>
        <template #cell-note="{ row }"
          ><span class="text-sm text-base-content/80">{{ row.note ?? '' }}</span></template
        >
        <template #cell-added="{ row }"
          ><span class="text-sm whitespace-nowrap">{{ formatDate(row.created_at) }}</span></template
        >
        <template #cell-actions="{ row }">
          <div v-if="isAdmin && row.reason !== 'complaint'" class="flex justify-end">
            <button
              type="button"
              class="btn btn-square btn-ghost btn-sm"
              :aria-label="`Remove ${row.email}`"
              @click="onRemove(row)"
            >
              <Trash2 class="size-4" aria-hidden="true" />
            </button>
          </div>
        </template>
      </DataTable>
      <p v-if="suppressions?.next_after" class="text-sm text-base-content/80">
        Showing the first {{ ROWS_SHOWN }}. Search to find others.
      </p>
    </PageSection>

    <FormPanel
      :open="panelOpen"
      title="Suppress address"
      description="No list in this workspace mails it, whatever its subscriptions say."
      submit-label="Suppress"
      :pending="add.asyncStatus.value === 'loading'"
      :error="add.error.value"
      @close="panelOpen = false"
      @submit="onAdd"
    >
      <Field label="Email">
        <input
          v-model="email"
          type="email"
          name="suppression-email"
          required
          class="input w-full"
        />
      </Field>
      <Field label="Note" hint="Optional. Why, for the team.">
        <input v-model="note" name="suppression-note" class="input w-full" />
      </Field>
    </FormPanel>

    <FormPanel
      :open="addressMode !== null"
      variant="dialog"
      :title="addressMode === 'erase' ? 'Erase address' : 'Export address'"
      :description="
        addressMode === 'erase'
          ? 'For an erasure request: removes the address from every list and the send log of this workspace.'
          : 'For an access request: downloads everything mailing holds on the address in this workspace, as JSON.'
      "
      :submit-label="addressMode === 'erase' ? 'Erase' : 'Download'"
      :pending="addressPending"
      @close="addressMode = null"
      @submit="onAddress"
    >
      <Field label="Email">
        <input v-model="address" type="email" name="address-email" required class="input w-full" />
      </Field>
    </FormPanel>
  </Page>
</template>
