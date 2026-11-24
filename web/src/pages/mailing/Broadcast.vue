<script setup lang="ts">
import { refDebounced, useIntervalFn } from '@vueuse/core'
import {
  CalendarClock,
  CalendarX,
  FlaskConical,
  Inbox,
  Play,
  RotateCw,
  Send,
  Trash2,
} from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import StatusBadge from '@/components/mailing/StatusBadge.vue'
import Card from '@/components/ui/Card.vue'
import DataTable from '@/components/ui/DataTable.vue'
import DescriptionList from '@/components/ui/DescriptionList.vue'
import DetailActions from '@/components/ui/DetailActions.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'
import { runAction } from '@/lib/action'
import { errorMessage } from '@/lib/errors'
import { formatDate, formatDateTime, formatNumber } from '@/lib/format'
import { toast } from '@/lib/toast'
import { useMe } from '@/queries/auth'
import {
  type Preview,
  ROWS_SHOWN,
  useBroadcast,
  useCancelBroadcast,
  useDeleteBroadcast,
  useDeliveries,
  useMailingList,
  usePreview,
  useResendBroadcast,
  useResumeBroadcast,
  useSendBroadcast,
  useTestBroadcast,
  useUpdateBroadcast,
} from '@/queries/mailing'

const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  broadcastId: string
}>()
const router = useRouter()
const {
  data: broadcast,
  error: broadcastError,
  refetch: refetchBroadcast,
  asyncStatus: broadcastStatus,
} = useBroadcast(
  () => props.workspaceId,
  () => props.broadcastId,
)
const { data: list } = useMailingList(
  () => props.workspaceId,
  () => broadcast.value?.list_id ?? null,
)
const isDraft = computed(() => broadcast.value?.status === 'draft')
const isScheduled = computed(() => broadcast.value?.status === 'scheduled')
const isPaused = computed(() => broadcast.value?.status === 'paused')
const isSending = computed(() => broadcast.value?.status === 'sending')
// A scheduled broadcast is still unsent: no log yet, and no edits.
const hasLog = computed(() => !isDraft.value && !isScheduled.value)
const {
  data: deliveries,
  isPending: deliveriesPending,
  error: deliveriesError,
  refetch: refetchDeliveries,
  asyncStatus: deliveriesStatus,
} = useDeliveries(
  () => props.workspaceId,
  () => props.broadcastId,
)
// The fan-out marks the broadcast sent once every mail is queued, before
// the mails go out, so the log polls on its own while any row is queued.
// It never cancels a refetch in flight: Send awaits its own invalidation of
// the log and would report the cancel as a failure.
const LOG_POLL_MS = 2_000
const logPoll = useIntervalFn(
  () => {
    if (deliveriesStatus.value !== 'loading') void refetchDeliveries()
  },
  LOG_POLL_MS,
  { immediate: false },
)
// The failed count on the Resend button follows the log.
watch(deliveries, () => {
  if (hasLog.value && broadcastStatus.value !== 'loading') void refetchBroadcast()
})
// A paused broadcast's queued rows wait for Resume, so the log stops polling.
watch(
  () =>
    hasLog.value &&
    !isPaused.value &&
    (isSending.value || (deliveries.value?.items ?? []).some((d) => d.status === 'queued')),
  (waiting) => (waiting ? logPoll.resume() : logPoll.pause()),
  { immediate: true },
)

const subject = ref('')
const tag = ref('')
const body = ref('')
// As in Script.vue: a refetch only replaces fields the user hasn't edited
// since the last fill, so a save or a poll never loses typing.
let synced: typeof broadcast.value
watch(
  broadcast,
  (loaded) => {
    if (!loaded) return
    const fresh = synced?.id !== loaded.id
    if (fresh || subject.value === synced?.subject) subject.value = loaded.subject
    if (fresh || tag.value === (synced?.tag ?? '')) tag.value = loaded.tag ?? ''
    if (fresh || body.value === synced?.body_md) body.value = loaded.body_md
    synced = loaded
  },
  { immediate: true },
)
watch(list, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(
      `/w/${props.workspaceId}/p/${loaded.project_id}/mailing/broadcasts/${props.broadcastId}`,
    )
  }
})

const dirty = computed(
  () =>
    broadcast.value !== undefined &&
    (subject.value.trim() !== broadcast.value.subject ||
      tag.value.trim() !== (broadcast.value.tag ?? '') ||
      body.value !== broadcast.value.body_md),
)

const update = useUpdateBroadcast()
const remove = useDeleteBroadcast()
const send = useSendBroadcast()
const resend = useResendBroadcast()
const cancel = useCancelBroadcast()
const resume = useResumeBroadcast()
const {
  mutateAsync: sendTest,
  reset: resetTest,
  asyncStatus: testStatus,
  error: testError,
} = useTestBroadcast()
const previewer = usePreview()

const preview = ref<Preview | null>(null)
const previewError = ref<unknown>(null)
const typed = computed(() => ({ subject: subject.value, body: body.value }))
const settled = refDebounced(typed, 400)
let previewRun = 0
watch(
  [settled, () => broadcast.value?.list_id],
  async ([text, listId]) => {
    if (!listId || text.subject.trim() === '') return
    const run = ++previewRun
    try {
      const rendered = await previewer.mutateAsync({
        path: { workspace_id: props.workspaceId },
        body: { list_id: listId, subject: text.subject, body_md: text.body },
      })
      if (run !== previewRun) return
      preview.value = rendered
      previewError.value = null
    } catch (error) {
      if (run === previewRun) previewError.value = error
    }
  },
  { immediate: true },
)

const audience = computed(() => {
  const name = list.value?.name ?? 'the list'
  const wanted = tag.value.trim()
  if (wanted !== '') return `subscribers of ${name} tagged “${wanted}”`
  const count = list.value?.counts?.subscribed
  return count === undefined
    ? `every subscriber of ${name}`
    : `${formatNumber(count)} ${count === 1 ? 'subscriber' : 'subscribers'} of ${name}`
})

const failed = computed(() => broadcast.value?.deliveries?.failed ?? 0)
const recipientsText = computed(() => {
  const current = broadcast.value
  if (!current || current.status === 'draft' || current.status === 'scheduled') return '—'
  const total = formatNumber(current.recipients)
  return failed.value > 0 ? `${total}, ${formatNumber(failed.value)} failed` : total
})

const facts = computed(() => {
  const current = broadcast.value
  if (!current) return []
  return [
    { key: 'list', label: 'List', value: list.value?.name ?? '' },
    { key: 'status', label: 'Status' },
    { key: 'to', label: 'To', value: current.tag ? `Tagged “${current.tag}”` : 'Every subscriber' },
    { key: 'recipients', label: 'Recipients', value: recipientsText.value },
    current.status === 'scheduled'
      ? { key: 'sent', label: 'Sends', value: formatDateTime(current.send_at) }
      : { key: 'sent', label: 'Sent', value: formatDate(current.sent_at) },
  ]
})

function save() {
  const wanted = tag.value.trim()
  return runAction({
    run: () =>
      update.mutateAsync({
        path: { workspace_id: props.workspaceId, broadcast_id: props.broadcastId },
        body: {
          subject: subject.value.trim(),
          body_md: body.value,
          tag: wanted === '' ? null : wanted,
        },
      }),
    success: 'Draft saved',
  })
}

const me = useMe()
const testOpen = ref(false)
const testTo = ref('')

function openTest() {
  testTo.value = me.data.value?.email ?? ''
  resetTest()
  testOpen.value = true
}

async function onTest() {
  await sendTest({
    path: { workspace_id: props.workspaceId, broadcast_id: props.broadcastId },
    body: { to: testTo.value },
  })
  testOpen.value = false
  toast(`Test queued for ${testTo.value}`)
}

function onSend() {
  return runAction({
    confirm: {
      title: `Send “${broadcast.value?.subject ?? ''}”?`,
      message: `This mails ${audience.value} now. A sent broadcast can’t change.`,
      confirmLabel: 'Send now',
      danger: false,
    },
    run: () =>
      send.mutateAsync({
        path: { workspace_id: props.workspaceId, broadcast_id: props.broadcastId },
        body: {},
      }),
    success: 'Sending',
  })
}

const scheduleOpen = ref(false)
const scheduleAt = ref('')
const scheduling = computed(() => send.asyncStatus.value === 'loading')
const scheduleError = computed(() => send.error.value)

/** `datetime-local` wants local time without a zone: 2026-11-24T09:30. */
function localInput(date: Date) {
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`
}

function openSchedule() {
  const inAnHour = new Date(Date.now() + 60 * 60 * 1000)
  inAnHour.setMinutes(0, 0, 0)
  scheduleAt.value = localInput(inAnHour)
  send.reset()
  scheduleOpen.value = true
}

async function onSchedule() {
  const sendAt = new Date(scheduleAt.value).toISOString()
  await send.mutateAsync({
    path: { workspace_id: props.workspaceId, broadcast_id: props.broadcastId },
    body: { send_at: sendAt },
  })
  scheduleOpen.value = false
  toast(`Scheduled for ${formatDateTime(sendAt)}`)
}

function onCancel() {
  return runAction({
    run: () =>
      cancel.mutateAsync({
        path: { workspace_id: props.workspaceId, broadcast_id: props.broadcastId },
      }),
    success: 'Schedule cancelled; it is a draft again',
  })
}

function onResume() {
  return runAction({
    confirm: {
      title: 'Resume sending?',
      message:
        'Fix whatever made the mail fail first, or the broadcast pauses again after 20 failures.',
      confirmLabel: 'Resume',
      danger: false,
    },
    run: () =>
      resume.mutateAsync({
        path: { workspace_id: props.workspaceId, broadcast_id: props.broadcastId },
      }),
    success: 'Resumed',
  })
}

function onDelete() {
  return runAction({
    confirm: {
      title: `Delete “${broadcast.value?.subject ?? 'this draft'}”?`,
      message: 'The draft goes away for good.',
      confirmLabel: 'Delete draft',
    },
    run: async () => {
      await remove.mutateAsync({
        path: { workspace_id: props.workspaceId, broadcast_id: props.broadcastId },
      })
      return true
    },
    success: 'Draft deleted',
  }).then((deleted) => {
    if (deleted) {
      void router.push(`/w/${props.workspaceId}/p/${props.projectId}/mailing/broadcasts`)
    }
  })
}

const sendBlocked = computed(() => {
  if (dirty.value) return 'Save the draft first'
  if (body.value.trim() === '') return 'Write the body first'
  return undefined
})
const actions = computed(() => {
  if (isScheduled.value) {
    return [
      {
        label: 'Cancel schedule',
        icon: CalendarX,
        run: () => void onCancel(),
        disabled: cancel.asyncStatus.value === 'loading',
      },
    ]
  }
  if (isPaused.value) {
    return [
      {
        label: 'Resume',
        icon: Play,
        run: () => void onResume(),
        disabled: resume.asyncStatus.value === 'loading',
      },
    ]
  }
  return isDraft.value
    ? [
        { label: 'Test send', icon: FlaskConical, run: openTest, disabled: dirty.value },
        {
          label: 'Schedule',
          icon: CalendarClock,
          run: openSchedule,
          disabled: sendBlocked.value !== undefined,
        },
        {
          label: 'Send',
          icon: Send,
          run: () => void onSend(),
          disabled: sendBlocked.value !== undefined || send.asyncStatus.value === 'loading',
        },
      ]
    : broadcast.value?.status === 'sent'
      ? [
          {
            label:
              failed.value > 0
                ? `Resend to ${formatNumber(failed.value)} failed`
                : 'Resend to failed',
            icon: RotateCw,
            run: () => void onResend(),
            disabled: failed.value === 0 || resend.asyncStatus.value === 'loading',
          },
        ]
      : []
})

const description = computed(() => {
  switch (broadcast.value?.status) {
    case 'draft':
      return 'A draft. Save it, send yourself a test, then send it now or schedule it.'
    case 'scheduled':
      return 'Scheduled. Cancel the schedule to edit it again.'
    case 'paused':
      return 'Paused: too many mails failed in a row. Mail already sent stays sent.'
    default:
      return 'Sent mail can’t change. The log shows each recipient.'
  }
})

function onResend() {
  const count = failed.value
  return runAction({
    confirm: {
      title: `Resend to ${formatNumber(count)} failed ${count === 1 ? 'recipient' : 'recipients'}?`,
      message: 'Only mail that failed goes out again. Nobody gets it twice.',
      confirmLabel: 'Resend',
      danger: false,
    },
    run: () =>
      resend.mutateAsync({
        path: { workspace_id: props.workspaceId, broadcast_id: props.broadcastId },
      }),
    success: (report) =>
      report.requeued === 0
        ? 'Nothing had failed'
        : `Queued ${formatNumber(report.requeued)} again`,
  })
}

const columns = [
  { key: 'to', label: 'To' },
  { key: 'status', label: 'Status' },
  { key: 'attempts', label: 'Attempts' },
  { key: 'sent', label: 'Sent' },
  { key: 'error', label: 'Error' },
]
</script>

<template>
  <Page>
    <ErrorAlert v-if="broadcastError" :error="broadcastError" />
    <template v-else-if="broadcast">
      <PageHeader :title="broadcast.subject" :description="description">
        <DetailActions
          :actions="actions"
          :save="
            isDraft
              ? {
                  form: 'broadcast-form',
                  dirty,
                  pending: update.asyncStatus.value === 'loading',
                }
              : undefined
          "
        >
          <template v-if="isDraft" #menu>
            <MenuItem danger @select="onDelete">
              <Trash2 class="size-4" aria-hidden="true" />Delete draft
            </MenuItem>
          </template>
        </DetailActions>
      </PageHeader>

      <div v-if="isPaused" role="alert" class="alert alert-warning">
        <span>{{ broadcast.pause_reason }}. Fix the mail setup, then resume to send the rest.</span>
      </div>

      <DescriptionList :items="facts">
        <template #status><StatusBadge :status="broadcast.status" /></template>
      </DescriptionList>

      <Card v-if="isDraft" title="Message">
        <form id="broadcast-form" class="flex flex-col gap-4" @submit.prevent="save">
          <Field label="Subject" hint="{{ name }} fills in each subscriber’s name.">
            <input v-model="subject" name="broadcast-subject" required class="input w-full" />
          </Field>
          <Field label="Tag" hint="Optional. Only subscribers with this tag get it.">
            <input v-model="tag" name="broadcast-tag" class="input w-full" placeholder="vip" />
          </Field>
          <Field
            label="Body"
            hint="Markdown: **bold**, _italic_, [links](https://example.com), lists. {{ name }} and {{ email }} fill in."
          >
            <textarea
              v-model="body"
              name="broadcast-body"
              rows="14"
              class="textarea w-full font-mono text-sm"
            ></textarea>
          </Field>
          <p v-if="sendBlocked && dirty" class="text-sm text-base-content/70">
            {{ sendBlocked }} to send or test it.
          </p>
        </form>
      </Card>

      <Card
        title="Preview"
        :description="
          isDraft ? 'How the mail looks, with a sample name filled in.' : 'The mail as sent.'
        "
      >
        <p v-if="previewError" class="text-sm text-error">{{ errorMessage(previewError) }}</p>
        <iframe
          v-else-if="preview"
          title="Mail preview"
          sandbox=""
          :srcdoc="preview.html"
          class="h-128 w-full rounded-field border border-base-300 bg-base-100"
        ></iframe>
        <p v-else class="text-sm text-base-content/70">Add a subject to see the preview.</p>
      </Card>

      <PageSection v-if="hasLog" title="Send log">
        <template #description>
          The newest {{ ROWS_SHOWN }} recipients. Mail to people who left the list or are suppressed
          is skipped. Bounces and spam complaints show once the provider reports them.
        </template>
        <DataTable
          :columns="columns"
          :rows="deliveries?.items"
          :row-key="(d) => d.id"
          :loading="deliveriesPending"
          :error="deliveriesError"
          label="Send log"
          :empty="{
            icon: Inbox,
            title: isSending ? 'Queueing mail' : 'Nobody to send to',
            message: isSending
              ? 'Recipients show here as their mail is queued.'
              : 'The list had no subscribers for this broadcast.',
          }"
        >
          <template #cell-to="{ row }"
            ><span class="text-sm font-medium">{{ row.to_email }}</span></template
          >
          <template #cell-status="{ row }"
            ><span class="flex flex-wrap gap-1"
              ><StatusBadge :status="row.status" /><StatusBadge
                v-if="row.event"
                :status="row.event" /></span
          ></template>
          <template #cell-attempts="{ row }"
            ><span class="text-sm tabular-nums">{{ row.attempts }}</span
            ><span v-if="row.resends > 0" class="ms-2 badge badge-ghost badge-sm"
              >resent{{ row.resends > 1 ? ` ×${row.resends}` : '' }}</span
            ></template
          >
          <template #cell-sent="{ row }"
            ><span class="text-sm whitespace-nowrap">{{
              formatDateTime(row.sent_at)
            }}</span></template
          >
          <template #cell-error="{ row }"
            ><span class="text-sm text-base-content/80">{{ row.error ?? '' }}</span></template
          >
        </DataTable>
      </PageSection>
    </template>

    <FormPanel
      :open="scheduleOpen"
      variant="dialog"
      title="Schedule broadcast"
      :description="`It goes to ${audience} at this time, your local time. Until then it can’t change; cancel the schedule to edit it.`"
      submit-label="Schedule"
      :pending="scheduling"
      :error="scheduleError"
      @close="scheduleOpen = false"
      @submit="onSchedule"
    >
      <Field label="Send at">
        <input
          v-model="scheduleAt"
          type="datetime-local"
          name="broadcast-send-at"
          required
          class="input w-full"
        />
      </Field>
    </FormPanel>

    <FormPanel
      :open="testOpen"
      variant="dialog"
      title="Send a test"
      description="Tests go only to members of this workspace, at most 20 an hour."
      submit-label="Send test"
      :pending="testStatus === 'loading'"
      :error="testError"
      @close="testOpen = false"
      @submit="onTest"
    >
      <Field label="To">
        <input v-model="testTo" type="email" name="test-to" required class="input w-full" />
      </Field>
    </FormPanel>
  </Page>
</template>
