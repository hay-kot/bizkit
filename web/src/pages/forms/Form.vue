<script setup lang="ts">
import { Download, Eye, Inbox, Settings, ShieldAlert, Trash2 } from 'lucide-vue-next'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'

import FieldsEditor from '@/components/forms/FieldsEditor.vue'
import { fetchSnippet, htmlSnippet } from '@/components/forms/embed'
import PageHeader from '@/components/PageHeader.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import CopyButton from '@/components/ui/CopyButton.vue'
import DataTable from '@/components/ui/DataTable.vue'
import DetailPanel from '@/components/ui/DetailPanel.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Select from '@/components/ui/Select.vue'
import { runAction } from '@/lib/action'
import { formatDate } from '@/lib/format'
import type { BotCheck, FieldDef, FieldValue, Submission, SubmissionStatus } from '@/queries/forms'
import {
  downloadExport,
  SUBMISSIONS_SHOWN,
  useDeleteForm,
  useDeleteSubmission,
  useForm,
  useSubmissions,
  useUpdateForm,
  useUpdateSubmission,
} from '@/queries/forms'

const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  formId: string
}>()
const router = useRouter()
const { data: form, error: formError } = useForm(
  () => props.workspaceId,
  () => props.formId,
)
const status = ref<SubmissionStatus>('inbox')
const {
  data: submissions,
  isPending,
  error,
} = useSubmissions(
  () => props.workspaceId,
  () => props.formId,
  status,
)
const updateForm = useUpdateForm()
const deleteForm = useDeleteForm()
const updateSubmission = useUpdateSubmission()
const deleteSubmission = useDeleteSubmission()

const statuses = [
  { value: 'inbox' as const, label: 'Inbox' },
  { value: 'spam' as const, label: 'Spam' },
]
const botChecks = [
  { value: 'honeypot' as const, label: 'Honeypot', description: 'Works with any HTML form.' },
  {
    value: 'challenge' as const,
    label: 'Challenge',
    description: 'Proof of work and a minimum fill time. Needs the ALTCHA widget.',
  },
]

const columns = computed(() => [
  { key: 'received', label: 'Received' },
  { key: 'preview', label: 'Submission' },
  ...(status.value === 'spam' ? [{ key: 'reason', label: 'Reason' }] : []),
  { key: 'actions', label: 'Actions', hidden: true, class: 'w-32 text-right' },
])

function text(value: FieldValue | undefined): string {
  if (value === undefined) return ''
  return Array.isArray(value) ? value.join(', ') : value
}

function preview(submission: Submission): string {
  const names = form.value?.fields.length
    ? form.value.fields.map((f) => f.name)
    : Object.keys(submission.data)
  return names
    .map((name) => text(submission.data[name]))
    .filter((value) => value.length > 0)
    .slice(0, 3)
    .join(' · ')
}

function labelOf(name: string): string {
  return form.value?.fields.find((f) => f.name === name)?.label ?? name
}

const viewing = ref<Submission | null>(null)

function move(submission: Submission, to: SubmissionStatus) {
  return runAction({
    run: () =>
      updateSubmission.mutateAsync({
        path: {
          workspace_id: props.workspaceId,
          form_id: props.formId,
          submission_id: submission.id,
        },
        body: { status: to },
      }),
    success: to === 'spam' ? 'Moved to spam' : 'Moved to the inbox',
  }).then(() => {
    viewing.value = null
  })
}

function removeSubmission(submission: Submission) {
  return runAction({
    confirm: {
      title: 'Delete this submission?',
      message: 'You can’t undo this.',
      confirmLabel: 'Delete submission',
    },
    run: () =>
      deleteSubmission.mutateAsync({
        path: {
          workspace_id: props.workspaceId,
          form_id: props.formId,
          submission_id: submission.id,
        },
      }),
    success: 'Deleted the submission',
  }).then(() => {
    viewing.value = null
  })
}

function onExport() {
  return runAction({
    run: () => downloadExport(props.workspaceId, props.formId, status.value),
    success: (result) =>
      result.truncated
        ? `Exported ${result.filename} (newest rows only)`
        : `Exported ${result.filename}`,
  })
}

function onDeleteForm() {
  const name = form.value?.name ?? 'this form'
  return runAction({
    confirm: {
      title: `Delete ${name}?`,
      message: 'This removes the form and all its submissions. Sites posting to it get a 404.',
      confirmLabel: 'Delete form',
    },
    run: async () => {
      await deleteForm.mutateAsync({
        path: { workspace_id: props.workspaceId, form_id: props.formId },
      })
      return true
    },
    success: `Deleted ${name}`,
  }).then(async (deleted) => {
    if (deleted) await router.push(`/w/${props.workspaceId}/p/${props.projectId}/forms`)
  })
}

// Settings panel state, copied from the form when the panel opens.
const settingsOpen = ref(false)
const settings = ref({
  name: '',
  enabled: true,
  botCheck: 'honeypot' as BotCheck,
  minFillSeconds: 2,
  redirectUrl: '',
  notifyEmails: '',
  fields: [] as FieldDef[],
})

function openSettings() {
  const current = form.value
  if (!current) return
  settings.value = {
    name: current.name,
    enabled: current.enabled,
    botCheck: current.bot_check,
    minFillSeconds: current.min_fill_ms / 1000,
    redirectUrl: current.redirect_url ?? '',
    notifyEmails: current.notify_emails.join('\n'),
    fields: current.fields.map((f) => ({ ...f })),
  }
  updateForm.reset()
  settingsOpen.value = true
}

async function onSaveSettings() {
  const s = settings.value
  await updateForm.mutateAsync({
    path: { workspace_id: props.workspaceId, form_id: props.formId },
    body: {
      name: s.name,
      enabled: s.enabled,
      bot_check: s.botCheck,
      min_fill_ms: Math.round(s.minFillSeconds * 1000),
      redirect_url: s.redirectUrl.trim() === '' ? null : s.redirectUrl.trim(),
      notify_emails: s.notifyEmails
        .split(/[\s,]+/)
        .map((e) => e.trim())
        .filter((e) => e.length > 0),
      fields: s.fields.map((f) => ({ ...f, label: f.label || null })),
    },
  })
  settingsOpen.value = false
}

const html = computed(() => (form.value ? htmlSnippet(form.value) : ''))
const script = computed(() => (form.value ? fetchSnippet(form.value) : ''))
</script>

<template>
  <div>
    <PageHeader
      :section="projectName"
      :title="form?.name ?? 'Form'"
      :description="
        form && !form.enabled ? 'Disabled: submissions get a 404.' : 'Submissions from your site.'
      "
    >
      <button type="button" class="btn gap-1.5 btn-ghost btn-sm" @click="onExport">
        <Download class="size-4" aria-hidden="true" />Export CSV
      </button>
      <button type="button" class="btn gap-1.5 btn-ghost btn-sm" @click="openSettings">
        <Settings class="size-4" aria-hidden="true" />Settings
      </button>
      <button type="button" class="btn gap-1.5 btn-ghost btn-sm" @click="onDeleteForm">
        <Trash2 class="size-4" aria-hidden="true" />Delete
      </button>
    </PageHeader>

    <p v-if="formError" class="mb-4 text-sm text-error">This form doesn’t exist.</p>

    <section v-if="form" class="mb-6 rounded-box border border-base-300 bg-base-100 p-4">
      <h2 class="text-sm font-semibold">Endpoint</h2>
      <div class="mt-2 flex items-center gap-2">
        <code class="truncate text-sm" data-testid="form-endpoint">{{ form.endpoint }}</code>
        <CopyButton :text="form.endpoint" label="Copy endpoint" />
      </div>
      <details class="mt-3">
        <summary class="cursor-pointer text-sm font-medium">HTML form</summary>
        <div class="mt-2 flex items-start gap-2">
          <pre tabindex="0" class="flex-1 overflow-x-auto rounded-field bg-base-200 p-3 text-xs">{{
            html
          }}</pre>
          <CopyButton :text="html" label="Copy HTML" />
        </div>
        <p v-if="form.bot_check === 'challenge'" class="mt-2 text-xs text-base-content/80">
          The widget loads its challenge on page load (<code>auto="onload"</code>); timing counts
          from then.
        </p>
      </details>
      <details class="mt-2">
        <summary class="cursor-pointer text-sm font-medium">Post from script</summary>
        <div class="mt-2 flex items-start gap-2">
          <pre tabindex="0" class="flex-1 overflow-x-auto rounded-field bg-base-200 p-3 text-xs">{{
            script
          }}</pre>
          <CopyButton :text="script" label="Copy script" />
        </div>
      </details>
    </section>

    <div class="mb-3 flex items-center justify-between gap-2">
      <h2 class="text-sm font-semibold">Submissions</h2>
      <Select v-model="status" :options="statuses" label="Show" size="sm" name="status" />
    </div>

    <DataTable
      :columns="columns"
      :rows="submissions?.items"
      :row-key="(s) => s.id"
      :loading="isPending"
      :error="error"
      label="Submissions"
      :empty="{
        icon: status === 'spam' ? ShieldAlert : Inbox,
        title: status === 'spam' ? 'No spam' : 'No submissions yet',
        message:
          status === 'spam'
            ? 'Submissions that fail the bot check land here.'
            : 'Post your site’s form to the endpoint above.',
      }"
    >
      <template #cell-received="{ row }"
        ><span class="text-sm whitespace-nowrap">{{ formatDate(row.created_at) }}</span></template
      >
      <template #cell-preview="{ row }"
        ><span class="line-clamp-1 text-sm">{{ preview(row) || '(empty)' }}</span></template
      >
      <template #cell-reason="{ row }"
        ><span class="badge badge-ghost badge-sm">{{ row.spam_reason ?? 'marked' }}</span></template
      >
      <template #cell-actions="{ row }">
        <div class="flex justify-end gap-1">
          <button
            type="button"
            class="btn btn-square btn-ghost btn-sm"
            :aria-label="`View submission from ${formatDate(row.created_at)}`"
            @click="viewing = row"
          >
            <Eye class="size-4" aria-hidden="true" />
          </button>
          <button
            type="button"
            class="btn btn-square btn-ghost btn-sm"
            :aria-label="row.status === 'spam' ? 'Move to inbox' : 'Mark as spam'"
            @click="move(row, row.status === 'spam' ? 'inbox' : 'spam')"
          >
            <component
              :is="row.status === 'spam' ? Inbox : ShieldAlert"
              class="size-4"
              aria-hidden="true"
            />
          </button>
          <button
            type="button"
            class="btn btn-square btn-ghost btn-sm"
            aria-label="Delete submission"
            @click="removeSubmission(row)"
          >
            <Trash2 class="size-4" aria-hidden="true" />
          </button>
        </div>
      </template>
    </DataTable>

    <p v-if="submissions?.next_before" class="mt-2 text-sm text-base-content/80">
      Showing the newest {{ SUBMISSIONS_SHOWN }}. Export CSV for the rest.
    </p>

    <DetailPanel
      :open="viewing !== null"
      title="Submission"
      :description="viewing ? formatDate(viewing.created_at) : undefined"
      @close="viewing = null"
    >
      <dl v-if="viewing" class="flex flex-col gap-3 text-sm">
        <div v-for="(value, name) in viewing.data" :key="name">
          <dt class="font-medium">{{ labelOf(String(name)) }}</dt>
          <dd class="break-words whitespace-pre-wrap">{{ text(value) }}</dd>
        </div>
        <div v-if="viewing.spam_reason">
          <dt class="font-medium">Spam reason</dt>
          <dd>{{ viewing.spam_reason }}</dd>
        </div>
        <div v-if="viewing.referer">
          <dt class="font-medium">Sent from</dt>
          <dd class="break-all">{{ viewing.referer }}</dd>
        </div>
        <div v-if="viewing.user_agent">
          <dt class="font-medium">Browser</dt>
          <dd class="break-all text-base-content/80">{{ viewing.user_agent }}</dd>
        </div>
      </dl>
      <template #actions>
        <button
          v-if="viewing"
          type="button"
          class="btn btn-ghost btn-sm"
          @click="move(viewing, viewing.status === 'spam' ? 'inbox' : 'spam')"
        >
          {{ viewing.status === 'spam' ? 'Move to inbox' : 'Mark as spam' }}
        </button>
      </template>
    </DetailPanel>

    <FormPanel
      :open="settingsOpen"
      title="Form settings"
      submit-label="Save settings"
      :pending="updateForm.asyncStatus.value === 'loading'"
      :error="updateForm.error.value"
      @close="settingsOpen = false"
      @submit="onSaveSettings"
    >
      <Field label="Name">
        <input v-model="settings.name" name="settings-name" required class="input w-full" />
      </Field>
      <Checkbox v-model="settings.enabled" label="Accept submissions" />
      <Field label="Bot check">
        <Select
          v-model="settings.botCheck"
          :options="botChecks"
          label="Bot check"
          name="settings-bot-check"
        />
      </Field>
      <Field
        v-if="settings.botCheck === 'challenge'"
        label="Minimum fill time (seconds)"
        hint="Faster submissions count as spam. 0 turns timing off."
      >
        <input
          v-model.number="settings.minFillSeconds"
          type="number"
          min="0"
          max="600"
          step="0.5"
          name="settings-min-fill"
          class="input w-full"
        />
      </Field>
      <Field
        label="Redirect after submit"
        hint="Optional. Where browsers go after a plain form post."
      >
        <input
          v-model="settings.redirectUrl"
          type="url"
          name="settings-redirect"
          class="input w-full"
          placeholder="https://example.com/thanks"
        />
      </Field>
      <Field label="Notify" hint="Email addresses, one per line. At most 10.">
        <textarea
          v-model="settings.notifyEmails"
          name="settings-notify"
          rows="2"
          class="textarea w-full"
        ></textarea>
      </Field>
      <Field label="Fields" hint="Defined fields are checked; others are dropped.">
        <FieldsEditor v-model="settings.fields" />
      </Field>
    </FormPanel>
  </div>
</template>
