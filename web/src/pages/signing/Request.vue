<script setup lang="ts">
import {
  Ban,
  Check,
  Download,
  FileSignature,
  History,
  RotateCcw,
  Send,
  Trash2,
  UserPlus,
} from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import {
  fieldDrafts,
  KIND_LABELS,
  localKey,
  orderOf,
  saveDraft as saveSigningDraft,
  sameFields,
  sameSigners,
  signerDrafts,
  signersWithoutSignature,
  type FieldDraft,
  type SignerDraft,
} from '@/components/signing/draft'
import FieldBox from '@/components/signing/FieldBox.vue'
import FieldEditor from '@/components/signing/FieldEditor.vue'
import PdfPages from '@/components/signing/PdfPages.vue'
import { signerColor } from '@/components/signing/signerColors'
import StatusBadge from '@/components/signing/StatusBadge.vue'
import Card from '@/components/ui/Card.vue'
import DataTable from '@/components/ui/DataTable.vue'
import DescriptionList from '@/components/ui/DescriptionList.vue'
import DetailActions, { type DetailAction } from '@/components/ui/DetailActions.vue'
import List from '@/components/ui/List.vue'
import ListItem from '@/components/ui/ListItem.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'
import { useIsAdmin } from '@/composables/useIsAdmin'
import { runAction } from '@/lib/action'
import { formatDateTime, formatNumber } from '@/lib/format'
import {
  type AuditEvent,
  type EventKind,
  isSealing,
  MAX_ORDER,
  MAX_SIGNERS,
  requestFileUrl,
  type SigningField,
  type SigningRequestDetail,
  useDeleteSigningRequest,
  useRetrySigningRequest,
  useSaveSigningDraft,
  useSendSigningRequest,
  useSigningRequest,
  useVoidSigningRequest,
} from '@/queries/signing'

const props = defineProps<{
  workspaceId: string
  projectId: string
  requestId: string
}>()
const router = useRouter()
const isAdmin = useIsAdmin(() => props.workspaceId)
const {
  data: detail,
  error: detailError,
  isPending,
  pollGaveUp,
} = useSigningRequest(
  () => props.workspaceId,
  () => props.requestId,
)
const request = computed(() => detail.value?.request)
const isDraft = computed(() => request.value?.status === 'draft')
// The URL's project comes from the link, the request's from the server.
// Follow the server so the header, sidebar and back links match.
watch(request, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(`/w/${props.workspaceId}/p/${loaded.project_id}/signing/${loaded.id}`)
  }
})
const saveDraft = useSaveSigningDraft()
const send = useSendSigningRequest()
const voidRequest = useVoidSigningRequest()
const retry = useRetrySigningRequest()
const deleteRequest = useDeleteSigningRequest()
const path = computed(() => ({ workspace_id: props.workspaceId, request_id: props.requestId }))
const listPath = computed(() => `/w/${props.workspaceId}/p/${props.projectId}/signing`)
const originalUrl = computed(() => requestFileUrl(props.workspaceId, props.requestId, 'original'))
const signedUrl = computed(() => requestFileUrl(props.workspaceId, props.requestId, 'signed'))

// ------------------------------------------------------------ draft state

const signers = ref<SignerDraft[]>([])
const fields = ref<FieldDraft[]>([])

// As in Sequence.vue: a refetch only replaces what the sender hasn't
// edited since the last fill, so a save or a poll never loses edits.
let synced: SigningRequestDetail | undefined
function fill(loaded: SigningRequestDetail) {
  const fresh = synced?.request.id !== loaded.request.id
  if (fresh || (synced && sameSigners(signers.value, signerDrafts(synced)))) {
    signers.value = signerDrafts(loaded)
  }
  if (fresh || (synced && sameFields(fields.value, fieldDrafts(synced.fields)))) {
    fields.value = fieldDrafts(loaded.fields)
  }
  synced = loaded
}
watch(detail, (loaded) => loaded && fill(loaded), { immediate: true })

const signersDirty = computed(
  () => !!detail.value && !sameSigners(signers.value, signerDrafts(detail.value)),
)
const fieldsDirty = computed(
  () => !!detail.value && !sameFields(fields.value, fieldDrafts(detail.value.fields)),
)
const dirty = computed(() => isDraft.value && (signersDirty.value || fieldsDirty.value))
const unplaced = computed(() => signersWithoutSignature(signers.value, fields.value))
const unplacedNames = computed(() =>
  unplaced.value.map((s) => s.name.trim() || `signer ${signers.value.indexOf(s) + 1}`).join(', '),
)

function addSigner() {
  signers.value.push({ key: localKey('signer'), name: '', email: '', signing_order: 1 })
}

function removeSigner(signer: SignerDraft) {
  const count = fields.value.filter((f) => f.signerKey === signer.key).length
  const drop = () => {
    signers.value = signers.value.filter((s) => s.key !== signer.key)
    fields.value = fields.value.filter((f) => f.signerKey !== signer.key)
  }
  if (count === 0) {
    drop()
    return
  }
  const who = signer.name.trim() || 'this signer'
  void runAction({
    confirm: {
      title: `Remove ${who}?`,
      message: `Their ${count === 1 ? 'field goes' : `${count} fields go`} too, once you save.`,
      confirmLabel: 'Remove signer',
    },
    run: async () => drop(),
  })
}

const editor = ref<InstanceType<typeof FieldEditor>>()
const saving = computed(() => saveDraft.asyncStatus.value === 'loading')

/** Saves the draft; gives true when it saved. The editor stays locked meanwhile. */
async function save(): Promise<boolean> {
  const result = await runAction({
    run: () =>
      saveDraft.mutateAsync({
        workspaceId: props.workspaceId,
        requestId: props.requestId,
        save: (api) => saveSigningDraft(api, signers.value, fields.value, signersDirty.value),
      }),
    success: 'Draft saved',
  })
  if (!result) return false
  synced = result.detail
  signers.value = signerDrafts(result.detail)
  fields.value = fieldDrafts(result.detail.fields)
  editor.value?.remap(result.ids)
  return true
}

// ------------------------------------------------------------ actions

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

function clientText(event: AuditEvent) {
  if (!event.ip && !event.user_agent) return undefined
  return [event.ip ?? 'unknown address', event.user_agent].filter(Boolean).join(', ')
}

const facts = computed(() => {
  const r = request.value
  if (!r) return []
  return [
    { key: 'status', label: 'Status' },
    { key: 'file', label: 'File', value: r.file_name },
    {
      key: 'pages',
      label: 'Pages',
      value: r.page_count === null || r.page_count === undefined ? '—' : formatNumber(r.page_count),
    },
    { key: 'sender', label: 'Sender', value: r.sender },
    r.completed_at
      ? { key: 'completed', label: 'Completed', value: formatDateTime(r.completed_at) }
      : r.voided_at
        ? { key: 'voided', label: 'Voided', value: formatDateTime(r.voided_at) }
        : { key: 'sent', label: 'Sent', value: formatDateTime(r.sent_at, 'Not yet') },
  ]
})

function groupCount() {
  return new Set(signers.value.map((s) => orderOf(s.signing_order))).size
}

const draftForm = ref<HTMLFormElement>()

function onSend() {
  const r = request.value
  // Save checks the form when it submits; Send saves without submitting,
  // so it checks first.
  if (!r || (draftForm.value && !draftForm.value.reportValidity())) return
  const count = signers.value.length
  const groups = groupCount()
  const first =
    groups > 1
      ? `This mails the first signing-order group its links; the ${groups - 1} later ${groups === 2 ? 'group gets' : 'groups get'} theirs once the group before signs.`
      : `This mails a private signing link to ${count} ${count === 1 ? 'signer' : 'signers'}.`
  void runAction({
    confirm: {
      title: `Send ${r.title}?`,
      message: `${first} The PDF, signers and fields can't change after that.`,
      confirmLabel: 'Send',
      danger: false,
    },
    run: async () => {
      if (dirty.value && !(await save())) return undefined
      return send.mutateAsync({ path: path.value })
    },
    success: (sent) => (sent ? 'Sent' : ''),
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
      disabled: send.asyncStatus.value === 'loading' || saving.value,
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

// ------------------------------------------------------------ after send

const signerColumns = [
  { key: 'name', label: 'Name' },
  { key: 'email', label: 'Email' },
  { key: 'order', label: 'Order' },
  { key: 'signed', label: 'Signed' },
]
const signerIndex = computed(() => new Map((request.value?.signers ?? []).map((s, i) => [s.id, i])))
const colorFor = (signerId: string) => signerColor(signerIndex.value.get(signerId) ?? 0)

function fieldValue(field: SigningField): string {
  const value = field.value
  if (!value) return field.label || KIND_LABELS[field.kind]
  switch (value.kind) {
    case 'signed':
      return 'Signed'
    case 'initialed':
      return 'Initialed'
    case 'date':
      return value.date
    case 'text':
      return value.text
    case 'checkbox':
      return value.checked ? 'Ticked' : 'Not ticked'
  }
}

function fieldLabel(field: SigningField): string {
  const kind = field.label ? `${KIND_LABELS[field.kind]} "${field.label}"` : KIND_LABELS[field.kind]
  const who = signerNames.value.get(field.signer_id) ?? 'a signer'
  const state = field.value ? `filled: ${fieldValue(field)}` : 'not filled yet'
  return `${kind} for ${who}, page ${field.page}, ${state}`
}
</script>

<template>
  <Page>
    <PageHeader :title="request?.title ?? ''" :description="request?.message || undefined">
      <DetailActions
        :actions="actions"
        :save="
          isDraft
            ? {
                form: 'signing-draft',
                dirty,
                pending: saving,
              }
            : undefined
        "
      >
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

    <ErrorAlert v-if="detailError" :error="detailError" />

    <template v-else-if="request">
      <DescriptionList :items="facts">
        <template #status><StatusBadge :status="request.status" /></template>
      </DescriptionList>

      <p
        v-if="pollGaveUp"
        role="status"
        class="rounded-box border border-base-300 bg-base-100 px-3 py-2 text-sm text-base-content/80"
      >
        Sealing is taking longer than usual. Reload this page later to see the result.
      </p>

      <div v-if="request.status === 'failed'" role="alert" class="alert alert-error">
        <span>Sealing failed: {{ request.failure }}. Retry the seal once the cause is fixed.</span>
      </div>

      <form
        v-if="isDraft"
        id="signing-draft"
        ref="draftForm"
        class="flex flex-col gap-8"
        @submit.prevent="save"
      >
        <Card
          title="Signers"
          description="Order 1 signs first. Equal numbers sign at the same time; the next group gets its links once everyone before it signed."
        >
          <fieldset class="flex flex-col gap-4" :disabled="saving">
            <div
              v-for="(signer, index) in signers"
              :key="signer.key"
              class="flex items-center gap-2"
              data-testid="signer-row"
            >
              <span
                class="size-3 shrink-0 rounded-full"
                :class="signerColor(index).swatch"
                aria-hidden="true"
              ></span>
              <input
                v-model="signer.name"
                :name="`signer-name-${index}`"
                required
                maxlength="120"
                aria-label="Signer name"
                class="input w-full input-sm"
                placeholder="Name"
              />
              <input
                v-model="signer.email"
                :name="`signer-email-${index}`"
                type="email"
                required
                aria-label="Signer email"
                class="input w-full input-sm"
                placeholder="Email"
              />
              <input
                v-model.number="signer.signing_order"
                :name="`signer-order-${index}`"
                type="number"
                min="1"
                :max="MAX_ORDER"
                step="1"
                required
                aria-label="Signing order"
                title="Signing order"
                class="input w-16 shrink-0 input-sm"
              />
              <button
                type="button"
                class="btn btn-square btn-ghost btn-sm"
                :disabled="signers.length === 1"
                :aria-label="`Remove signer ${index + 1}`"
                @click="removeSigner(signer)"
              >
                <Trash2 class="size-4" aria-hidden="true" />
              </button>
            </div>
            <div>
              <button
                type="button"
                class="btn gap-1.5 btn-ghost btn-sm"
                :disabled="signers.length >= MAX_SIGNERS"
                @click="addSigner"
              >
                <UserPlus class="size-4" aria-hidden="true" />Add signer
              </button>
            </div>
          </fieldset>
        </Card>

        <PageSection
          title="Fields"
          description="Place each signer's fields on the pages. Every signer needs at least one signature field before you send."
        >
          <p
            v-if="unplaced.length > 0"
            class="rounded-box border border-warning/50 bg-warning/10 px-3 py-2 text-sm"
            data-testid="unplaced"
          >
            No signature field yet for
            {{ unplacedNames }}.
          </p>
          <FieldEditor
            ref="editor"
            v-model="fields"
            :locked="saving"
            :url="originalUrl"
            :title="request.file_name"
            :signers="signers"
            :page-count="request.page_count ?? 1"
          />
        </PageSection>
      </form>

      <template v-else>
        <PageSection title="Signers">
          <DataTable
            :columns="signerColumns"
            :rows="request.signers"
            :row-key="(s) => s.id"
            label="Signers"
          >
            <template #cell-name="{ row }">
              <span class="flex items-center gap-2">
                <span
                  class="size-3 shrink-0 rounded-full"
                  :class="colorFor(row.id).swatch"
                  aria-hidden="true"
                ></span>
                <span class="font-medium break-words">{{ row.name }}</span>
              </span>
            </template>
            <template #cell-email="{ row }"
              ><span class="text-sm break-all">{{ row.email }}</span></template
            >
            <template #cell-order="{ row }"
              ><span class="text-sm tabular-nums">{{ row.signing_order }}</span></template
            >
            <template #cell-signed="{ row }">
              <span v-if="row.signed" class="inline-flex items-center gap-1 text-sm">
                <Check class="size-4 text-success" aria-hidden="true" />{{
                  formatDateTime(row.signed_at)
                }}
              </span>
              <span v-else class="badge badge-ghost badge-sm">pending</span>
            </template>
          </DataTable>
        </PageSection>

        <PageSection title="Documents">
          <div class="flex flex-wrap gap-2">
            <a
              v-if="request.status === 'completed'"
              class="btn gap-1.5 btn-primary btn-sm"
              :href="signedUrl"
              download
            >
              <Download class="size-4" aria-hidden="true" />Signed PDF
            </a>
            <a class="btn gap-1.5 btn-sm" :href="originalUrl" :download="request.file_name">
              <Download class="size-4" aria-hidden="true" />Original
            </a>
          </div>
        </PageSection>

        <PageSection title="Fields">
          <PdfPages :url="originalUrl" :title="request.file_name">
            <template #page="{ page }">
              <FieldBox
                v-for="field in detail!.fields.filter((f) => f.page === page)"
                :key="field.id"
                :rect="field"
              >
                <div
                  class="flex size-full items-center overflow-hidden rounded-sm border-2 px-1 text-xs font-medium text-base-content"
                  :class="[colorFor(field.signer_id).box, field.value ? '' : 'border-dashed']"
                  role="img"
                  :aria-label="fieldLabel(field)"
                  :title="fieldLabel(field)"
                >
                  <Check
                    v-if="field.value"
                    class="size-3.5 shrink-0 text-success"
                    aria-hidden="true"
                  />
                  <span class="truncate">{{
                    field.kind === 'checkbox' && !field.value ? '' : fieldValue(field)
                  }}</span>
                </div>
              </FieldBox>
            </template>
          </PdfPages>
        </PageSection>
      </template>

      <PageSection title="Audit trail">
        <List
          :items="events"
          :item-key="(e) => String(e.index)"
          :loading="isPending"
          label="Audit trail"
          ordered
          :empty="{ icon: History, title: 'No events', message: 'Nothing happened yet.' }"
        >
          <template #item="{ item }">
            <ListItem :title="eventText(item)">
              <template #leading>
                <FileSignature class="size-4 shrink-0 text-base-content/60" aria-hidden="true" />
              </template>
              <template v-if="item.detail || clientText(item)" #description>
                {{ [item.detail, clientText(item)].filter(Boolean).join(' · ') }}
              </template>
              <template #meta>
                <span class="whitespace-nowrap">{{ formatDateTime(item.at) }}</span>
              </template>
            </ListItem>
          </template>
        </List>
      </PageSection>
    </template>
  </Page>
</template>
