<script setup lang="ts">
import { Ban, Download, FileSignature, RotateCcw, Send, Trash2 } from 'lucide-vue-next'
import { computed, watch } from 'vue'
import { useRouter } from 'vue-router'

import StatusBadge from '@/components/signing/StatusBadge.vue'
import DataTable from '@/components/ui/DataTable.vue'
import DescriptionList from '@/components/ui/DescriptionList.vue'
import DetailActions, { type DetailAction } from '@/components/ui/DetailActions.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { runAction } from '@/lib/action'
import { formatDateTime } from '@/lib/format'
import {
  type AuditEvent,
  type DocumentVersion,
  type EventKind,
  downloadSigningDocument,
  isSealing,
  useDeleteSigningRequest,
  useRetrySigningRequest,
  useSendSigningRequest,
  useSigningRequest,
  useVoidSigningRequest,
} from '@/queries/signing'

const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  requestId: string
}>()
const router = useRouter()
const isAdmin = useIsAdmin(() => props.workspaceId)
const {
  data: detail,
  error: detailError,
  isPending,
} = useSigningRequest(
  () => props.workspaceId,
  () => props.requestId,
)
const request = computed(() => detail.value?.request)
// The URL's project comes from the link, the request's from the server.
// Follow the server so the header, sidebar and back links match.
watch(request, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(`/w/${props.workspaceId}/p/${loaded.project_id}/signing/${loaded.id}`)
  }
})
const send = useSendSigningRequest()
const voidRequest = useVoidSigningRequest()
const retry = useRetrySigningRequest()
const deleteRequest = useDeleteSigningRequest()
const path = computed(() => ({ workspace_id: props.workspaceId, request_id: props.requestId }))
const listPath = computed(() => `/w/${props.workspaceId}/p/${props.projectId}/signing`)

const signerColumns = [
  { key: 'name', label: 'Name' },
  { key: 'email', label: 'Email' },
  { key: 'signed', label: 'Signed' },
]
const eventColumns = [
  { key: 'at', label: 'When' },
  { key: 'what', label: 'What' },
  { key: 'client', label: 'Client' },
]

const EVENT_LABELS: Record<EventKind, string> = {
  created: 'Created',
  sent: 'Link sent',
  viewed: 'Opened the signing page',
  signed: 'Signed',
  completed: 'Sealed',
  voided: 'Voided',
  failed: 'Sealing failed',
}

const signerNames = computed(
  () => new Map((request.value?.signers ?? []).map((s) => [s.id, `${s.name} <${s.email}>`])),
)

function eventText(event: AuditEvent) {
  const who = event.signer_id ? signerNames.value.get(event.signer_id) : undefined
  return who ? `${EVENT_LABELS[event.kind]}: ${who}` : EVENT_LABELS[event.kind]
}

const facts = computed(() => {
  const r = request.value
  if (!r) return []
  return [
    { key: 'status', label: 'Status' },
    { key: 'file', label: 'File', value: r.file_name },
    { key: 'sender', label: 'Sender', value: r.sender },
    { key: 'created', label: 'Created', value: formatDateTime(r.created_at) },
    r.completed_at
      ? { key: 'completed', label: 'Completed', value: formatDateTime(r.completed_at) }
      : r.voided_at
        ? { key: 'voided', label: 'Voided', value: formatDateTime(r.voided_at) }
        : { key: 'sent', label: 'Sent', value: formatDateTime(r.sent_at, 'Not yet') },
  ]
})

function download(version: DocumentVersion) {
  void runAction({
    run: () => downloadSigningDocument(props.workspaceId, props.requestId, version),
  })
}

function onSend() {
  const r = request.value
  if (!r) return
  const count = r.signers.length
  void runAction({
    confirm: {
      title: `Send ${r.title}?`,
      message: `This mails a private signing link to ${count} ${count === 1 ? 'signer' : 'signers'}. The PDF and signers can't change after that.`,
      confirmLabel: 'Send',
    },
    run: () => send.mutateAsync({ path: path.value }),
    success: 'Sent',
  })
}

function onRetry() {
  void runAction({
    run: () => retry.mutateAsync({ path: path.value }),
    success: 'Sealing again',
  })
}

function onVoid() {
  const name = request.value?.title ?? 'this request'
  void runAction({
    confirm: {
      title: `Void ${name}?`,
      message: 'Signing links stop working. Signatures given so far stay in the audit trail.',
      confirmLabel: 'Void request',
      danger: true,
    },
    run: () => voidRequest.mutateAsync({ path: path.value }),
    success: `Voided ${name}`,
  })
}

function onDelete() {
  const name = request.value?.title ?? 'this request'
  return runAction({
    confirm: {
      title: `Delete ${name}?`,
      message: 'This removes the request, its PDF and its audit trail.',
      confirmLabel: 'Delete request',
    },
    run: async () => {
      await deleteRequest.mutateAsync({ path: path.value })
      return true
    },
    success: `Deleted ${name}`,
  }).then(async (deleted) => {
    if (deleted) await router.push(listPath.value)
  })
}

const actions = computed<DetailAction[]>(() => {
  const r = request.value
  if (!r) return []
  const list: DetailAction[] = []
  if (r.status === 'draft') {
    list.push({
      label: 'Send',
      icon: Send,
      run: onSend,
      disabled: send.asyncStatus.value === 'loading',
    })
  }
  if (r.status === 'failed') {
    list.push({
      label: 'Retry seal',
      icon: RotateCcw,
      run: onRetry,
      disabled: retry.asyncStatus.value === 'loading',
    })
  }
  if (r.status === 'completed') {
    list.push({ label: 'Signed PDF', icon: Download, run: () => download('signed') })
  }
  list.push({ label: 'Original', icon: Download, run: () => download('original') })
  return list
})
// Once everyone signed the seal is under way, and the API refuses a void.
const canVoid = computed(
  () =>
    request.value?.status === 'draft' ||
    (request.value?.status === 'sent' && !isSealing(request.value)),
)
const canDelete = computed(
  () => isAdmin.value && (request.value?.status === 'draft' || request.value?.status === 'voided'),
)
// Events have no ID; the trail only grows, so the position is stable.
const events = computed(() =>
  (detail.value?.events ?? []).map((event, index) => ({ ...event, index })),
)
</script>

<template>
  <Page>
    <PageHeader
      :section="projectName"
      :title="request?.title ?? 'Signature request'"
      :description="request?.message || undefined"
    >
      <DetailActions :actions="actions">
        <template v-if="canVoid || canDelete" #menu>
          <MenuItem v-if="canVoid" danger @select="onVoid">
            <Ban class="size-4" aria-hidden="true" />Void request
          </MenuItem>
          <MenuItem v-if="canDelete" danger @select="onDelete">
            <Trash2 class="size-4" aria-hidden="true" />Delete request
          </MenuItem>
        </template>
      </DetailActions>
    </PageHeader>

    <p v-if="detailError" class="text-sm text-error">This request doesn’t exist.</p>

    <template v-if="!detailError">
      <DescriptionList v-if="request" :items="facts">
        <template #status><StatusBadge :status="request.status" /></template>
      </DescriptionList>

      <div v-if="request?.status === 'failed'" role="alert" class="alert alert-error">
        <span>Sealing failed: {{ request.failure }}. Retry the seal once the cause is fixed.</span>
      </div>

      <PageSection title="Signers">
        <DataTable
          :columns="signerColumns"
          :rows="request?.signers"
          :row-key="(s) => s.id"
          :loading="isPending"
          label="Signers"
          :empty="{
            icon: FileSignature,
            title: 'No signers',
            message: 'This request has no signers.',
          }"
        >
          <template #cell-name="{ row }"
            ><span class="font-medium break-words">{{ row.name }}</span></template
          >
          <template #cell-email="{ row }"
            ><span class="text-sm break-all">{{ row.email }}</span></template
          >
          <template #cell-signed="{ row }">
            <span v-if="row.signed" class="text-sm">{{ formatDateTime(row.signed_at) }}</span>
            <span v-else class="badge badge-ghost badge-sm">pending</span>
          </template>
        </DataTable>
      </PageSection>

      <PageSection title="Audit trail">
        <DataTable
          :columns="eventColumns"
          :rows="events"
          :row-key="(e) => String(e.index)"
          :loading="isPending"
          label="Audit trail"
          :empty="{ icon: FileSignature, title: 'No events', message: 'Nothing happened yet.' }"
        >
          <template #cell-at="{ row }"
            ><span class="text-sm whitespace-nowrap">{{ formatDateTime(row.at) }}</span></template
          >
          <template #cell-what="{ row }">
            <div class="text-sm">{{ eventText(row) }}</div>
            <div v-if="row.detail" class="text-xs break-all text-base-content/70">
              {{ row.detail }}
            </div>
          </template>
          <template #cell-client="{ row }">
            <div v-if="row.ip || row.user_agent" class="text-xs text-base-content/70">
              <div>{{ row.ip ?? 'unknown address' }}</div>
              <div class="break-all">{{ row.user_agent }}</div>
            </div>
            <span v-else class="text-sm text-base-content/60">—</span>
          </template>
        </DataTable>
      </PageSection>
    </template>
  </Page>
</template>
