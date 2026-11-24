<script setup lang="ts">
import { Download, Inbox, Settings, ShieldAlert, Trash2 } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import EntitySections from '@/components/EntitySections.vue'
import FieldsEditor from '@/components/forms/FieldsEditor.vue'
import { fetchSnippet, htmlSnippet } from '@/components/forms/embed'
import Avatar from '@/components/ui/Avatar.vue'
import Card from '@/components/ui/Card.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import CopyButton from '@/components/ui/CopyButton.vue'
import DetailActions from '@/components/ui/DetailActions.vue'
import DetailPanel from '@/components/ui/DetailPanel.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import List from '@/components/ui/List.vue'
import ListItem from '@/components/ui/ListItem.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'
import Select from '@/components/ui/Select.vue'
import { runAction } from '@/lib/action'
import { formatDate, formatRelative } from '@/lib/format'
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
  formId: string
}>()
const router = useRouter()
const { data: form, error: formError } = useForm(
  () => props.workspaceId,
  () => props.formId,
)
// The URL's project comes from the link, the form's from the server. Follow
// the server so the header, sidebar and back links match the form.
watch(form, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(`/w/${props.workspaceId}/p/${loaded.project_id}/forms/${loaded.id}`)
  }
})
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

function text(value: FieldValue | undefined): string {
  if (value === undefined) return ''
  return Array.isArray(value) ? value.join(', ') : value
}

function fieldNames(submission: Submission): string[] {
  return form.value?.fields.length
    ? form.value.fields.map((f) => f.name)
    : Object.keys(submission.data)
}

/**
 * The field that says who sent it: an email field, else one named like a
 * name, else none. Forms are free-form, so this is a guess.
 */
function senderField(submission: Submission): string | undefined {
  const filled = fieldNames(submission).filter((name) => text(submission.data[name]).length > 0)
  const email = form.value?.fields.find((f) => f.kind === 'email' && filled.includes(f.name))
  return email?.name ?? filled.find((name) => /e-?mail|name/i.test(name))
}

function sender(submission: Submission): string {
  const field = senderField(submission)
  return field ? text(submission.data[field]) : 'Anonymous'
}

function preview(submission: Submission): string {
  const skip = senderField(submission)
  return fieldNames(submission)
    .filter((name) => name !== skip)
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
  <Page>
    <PageHeader
      :title="form?.name ?? ''"
      :description="
        form && !form.enabled ? 'Disabled: submissions get a 404.' : 'Submissions from your site.'
      "
    >
      <DetailActions
        :actions="[
          { label: 'Export CSV', icon: Download, run: onExport },
          { label: 'Settings', icon: Settings, run: openSettings },
        ]"
      >
        <template #menu>
          <MenuItem danger @select="onDeleteForm">
            <Trash2 class="size-4" aria-hidden="true" />Delete form
          </MenuItem>
        </template>
      </DetailActions>
    </PageHeader>

    <p v-if="formError" class="text-sm text-error">This form doesn’t exist.</p>

    <Card v-if="form" title="Endpoint">
      <div class="flex items-center gap-2">
        <code class="truncate text-sm" data-testid="form-endpoint">{{ form.endpoint }}</code>
        <CopyButton :text="form.endpoint" label="Copy endpoint" />
      </div>
      <details>
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
      <details>
        <summary class="cursor-pointer text-sm font-medium">Post from script</summary>
        <div class="mt-2 flex items-start gap-2">
          <pre tabindex="0" class="flex-1 overflow-x-auto rounded-field bg-base-200 p-3 text-xs">{{
            script
          }}</pre>
          <CopyButton :text="script" label="Copy script" />
        </div>
      </details>
    </Card>

    <PageSection title="Submissions">
      <template #actions>
        <Select v-model="status" :options="statuses" label="Show" size="toolbar" name="status" />
      </template>

      <List
        :items="submissions?.items"
        :item-key="(s) => s.id"
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
        <template #item="{ item }">
          <ListItem
            :title="sender(item)"
            :description="preview(item) || '(empty)'"
            @select="viewing = item"
          >
            <template #leading><Avatar :name="sender(item)" /></template>
            <template #meta>
              <span class="flex items-center gap-2 whitespace-nowrap">
                <span v-if="item.status === 'spam'" class="badge badge-ghost badge-sm">{{
                  item.spam_reason ?? 'marked'
                }}</span>
                {{ formatRelative(item.created_at) }}
              </span>
            </template>
            <template #trailing>
              <button
                type="button"
                class="btn btn-square btn-ghost btn-sm"
                :aria-label="item.status === 'spam' ? 'Move to inbox' : 'Mark as spam'"
                @click="move(item, item.status === 'spam' ? 'inbox' : 'spam')"
              >
                <component
                  :is="item.status === 'spam' ? Inbox : ShieldAlert"
                  class="size-4"
                  aria-hidden="true"
                />
              </button>
              <button
                type="button"
                class="btn btn-square btn-ghost btn-sm"
                aria-label="Delete submission"
                @click="removeSubmission(item)"
              >
                <Trash2 class="size-4" aria-hidden="true" />
              </button>
            </template>
          </ListItem>
        </template>
      </List>

      <p v-if="submissions?.next_before" class="text-sm text-base-content/80">
        Showing the newest {{ SUBMISSIONS_SHOWN }}. Export CSV for the rest.
      </p>
    </PageSection>

    <EntitySections
      v-if="form"
      :workspace-id="workspaceId"
      :project-id="projectId"
      kind="forms.form"
      :entity-id="form.id"
    />

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
  </Page>
</template>
