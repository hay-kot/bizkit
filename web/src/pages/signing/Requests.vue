<script setup lang="ts">
import { FileSignature, Plus, Trash2, UserPlus } from 'lucide-vue-next'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'

import StatusBadge from '@/components/signing/StatusBadge.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FileInput from '@/components/ui/FileInput.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import { useCreateIntent } from '@/composables/useCreateIntent'
import { fileToBase64 } from '@/lib/files'
import { formatRelative } from '@/lib/format'
import {
  MAX_PDF_BYTES,
  MAX_SIGNERS,
  type NewSigner,
  type SigningRequest,
  useCreateSigningRequest,
  useSigningRequests,
} from '@/queries/signing'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const router = useRouter()
const {
  data: requests,
  isPending,
  error,
} = useSigningRequests(
  () => props.workspaceId,
  () => props.projectId,
)
const create = useCreateSigningRequest()
const panelOpen = ref(false)
useCreateIntent(openPanel)

const title = ref('')
const message = ref('')
const file = ref<File | null>(null)
const signers = ref<NewSigner[]>([])
const fileError = ref<Error | null>(null)
const formError = computed(() => fileError.value ?? create.error.value)

const columns = [
  { key: 'title', label: 'Title' },
  { key: 'status', label: 'Status' },
  { key: 'signers', label: 'Signed' },
  { key: 'created', label: 'Created' },
]

function openPanel() {
  title.value = ''
  message.value = ''
  file.value = null
  signers.value = [{ name: '', email: '' }]
  fileError.value = null
  create.reset()
  panelOpen.value = true
}

function requestPath(requestId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/signing/${requestId}`
}

function signedCount(request: SigningRequest) {
  return `${request.signers.filter((s) => s.signed).length} of ${request.signers.length}`
}

function addSigner() {
  signers.value.push({ name: '', email: '' })
}

function removeSigner(index: number) {
  signers.value.splice(index, 1)
}

async function onCreate() {
  fileError.value = null
  const pdf = file.value
  if (!pdf) {
    fileError.value = new Error('Choose a PDF to send.')
    return
  }
  if (pdf.size > MAX_PDF_BYTES) {
    fileError.value = new Error('The PDF must be at most 700 KiB.')
    return
  }
  const created = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: {
      project_id: props.projectId,
      title: title.value,
      message: message.value,
      file_name: pdf.name,
      pdf_base64: await fileToBase64(pdf),
      signers: signers.value.map((s) => ({ name: s.name.trim(), email: s.email.trim() })),
    },
  })
  panelOpen.value = false
  await router.push(requestPath(created.id))
}
</script>

<template>
  <Page>
    <PageHeader
      :section="projectName"
      title="Signing"
      description="Send a PDF for signature. Each signer gets a private link, and everyone gets the sealed copy."
    >
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New request
      </button>
    </PageHeader>

    <DataTable
      :columns="columns"
      :rows="requests?.items"
      :row-key="(r) => r.id"
      :loading="isPending"
      :error="error"
      :empty="{
        icon: FileSignature,
        title: 'No signature requests yet',
        message: 'Upload a PDF, name the signers, and send it.',
      }"
    >
      <template #cell-title="{ row }">
        <RouterLink :to="requestPath(row.id)" class="link font-medium link-hover">
          {{ row.title }}
        </RouterLink>
        <div class="text-xs text-base-content/70">{{ row.file_name }}</div>
      </template>
      <template #cell-status="{ row }"><StatusBadge :status="row.status" /></template>
      <template #cell-signers="{ row }"
        ><span class="text-sm tabular-nums">{{ signedCount(row) }}</span></template
      >
      <template #cell-created="{ row }"
        ><span class="text-sm">{{ formatRelative(row.created_at) }}</span></template
      >
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">New request</button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      title="New signature request"
      description="It starts as a draft. Nothing is sent until you press Send."
      submit-label="Create draft"
      :pending="create.asyncStatus.value === 'loading'"
      :error="formError"
      @close="panelOpen = false"
      @submit="onCreate"
    >
      <Field label="Title" hint="What signers see as the subject.">
        <input
          v-model="title"
          name="signing-title"
          required
          maxlength="120"
          class="input w-full"
          placeholder="Services agreement"
        />
      </Field>
      <Field label="PDF" hint="At most 700 KiB. Encrypted PDFs can't be signed.">
        <FileInput v-model="file" name="signing-pdf" accept="application/pdf,.pdf" required />
      </Field>
      <Field label="Message" hint="Optional. Shown on the signing page.">
        <textarea
          v-model="message"
          name="signing-message"
          maxlength="2000"
          rows="3"
          class="textarea w-full"
        ></textarea>
      </Field>
      <fieldset class="flex flex-col gap-2">
        <legend class="mb-1.5 text-sm font-medium">Signers</legend>
        <div
          v-for="(signer, index) in signers"
          :key="index"
          class="flex items-center gap-2"
          data-testid="signer-row"
        >
          <input
            v-model="signer.name"
            :name="`signer-name-${index}`"
            required
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
          <button
            type="button"
            class="btn btn-square btn-ghost btn-sm"
            :disabled="signers.length === 1"
            :aria-label="`Remove signer ${index + 1}`"
            @click="removeSigner(index)"
          >
            <Trash2 class="size-4" aria-hidden="true" />
          </button>
        </div>
        <button
          type="button"
          class="btn gap-1.5 self-start btn-ghost btn-sm"
          :disabled="signers.length >= MAX_SIGNERS"
          @click="addSigner"
        >
          <UserPlus class="size-4" aria-hidden="true" />Add signer
        </button>
      </fieldset>
    </FormPanel>
  </Page>
</template>
