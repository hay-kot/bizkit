<script setup lang="ts">
import { ArrowDown, ArrowUp, Inbox, LogOut, Plus, Trash2, UserPlus, Users } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import EntitySections from '@/components/EntitySections.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import StatusBadge from '@/components/mailing/StatusBadge.vue'
import Card from '@/components/ui/Card.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import DataTable from '@/components/ui/DataTable.vue'
import DescriptionList from '@/components/ui/DescriptionList.vue'
import DetailActions from '@/components/ui/DetailActions.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'
import Select from '@/components/ui/Select.vue'
import { runAction } from '@/lib/action'
import { formatDate, formatDateTime, formatNumber } from '@/lib/format'
import {
  type Enrollment,
  ROWS_SHOWN,
  type Sequence,
  type SequenceTrigger,
  useContacts,
  useDeleteSequence,
  useEnroll,
  useEnrollments,
  useExitEnrollment,
  useMailingList,
  useSequence,
  useSequenceDeliveries,
  useUpdateSequence,
} from '@/queries/mailing'

const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  sequenceId: string
}>()
const router = useRouter()
const { data: sequence, error: sequenceError } = useSequence(
  () => props.workspaceId,
  () => props.sequenceId,
)
const { data: list } = useMailingList(
  () => props.workspaceId,
  () => sequence.value?.list_id ?? null,
)
watch(list, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(
      `/w/${props.workspaceId}/p/${loaded.project_id}/mailing/sequences/${props.sequenceId}`,
    )
  }
})
const {
  data: enrollments,
  isPending: enrollmentsPending,
  error: enrollmentsError,
} = useEnrollments(
  () => props.workspaceId,
  () => props.sequenceId,
)
const {
  data: deliveries,
  isPending: deliveriesPending,
  error: deliveriesError,
} = useSequenceDeliveries(
  () => props.workspaceId,
  () => props.sequenceId,
)

/** A step as the editor holds it; `key` stays put while steps move. */
interface DraftStep {
  key: string
  id?: string
  delay_hours: number
  subject: string
  body_md: string
}

const name = ref('')
const trigger = ref<SequenceTrigger>('on_join')
const enabled = ref(false)
const steps = ref<DraftStep[]>([])
let nextKey = 0

function draftSteps(loaded: Sequence): DraftStep[] {
  return loaded.steps.map((step) => ({
    key: step.id,
    id: step.id,
    delay_hours: step.delay_hours,
    subject: step.subject,
    body_md: step.body_md,
  }))
}

function sameSteps(a: DraftStep[], b: DraftStep[]): boolean {
  return (
    a.length === b.length &&
    a.every(
      (step, i) =>
        step.id === b[i]?.id &&
        step.delay_hours === b[i]?.delay_hours &&
        step.subject.trim() === b[i]?.subject.trim() &&
        step.body_md === b[i]?.body_md,
    )
  )
}

// As in Broadcast.vue: a refetch only replaces fields the user hasn't
// edited since the last fill, so a save never loses typing.
let synced: Sequence | undefined
watch(
  sequence,
  (loaded) => {
    if (!loaded) return
    const fresh = synced?.id !== loaded.id
    if (fresh || name.value === synced?.name) name.value = loaded.name
    if (fresh || trigger.value === synced?.trigger) trigger.value = loaded.trigger
    if (fresh || enabled.value === synced?.enabled) enabled.value = loaded.enabled
    if (fresh || (synced && sameSteps(steps.value, draftSteps(synced)))) {
      steps.value = draftSteps(loaded)
    }
    synced = loaded
  },
  { immediate: true },
)

const dirty = computed(() => {
  const current = sequence.value
  if (!current) return false
  return (
    name.value.trim() !== current.name ||
    trigger.value !== current.trigger ||
    enabled.value !== current.enabled ||
    !sameSteps(steps.value, draftSteps(current))
  )
})

const triggerOptions: { value: SequenceTrigger; label: string }[] = [
  { value: 'on_join', label: 'When someone subscribes' },
  { value: 'manual', label: 'Only when enrolled by hand or the API' },
]

function addStep() {
  steps.value.push({
    key: `new-${nextKey++}`,
    delay_hours: steps.value.length === 0 ? 0 : 24,
    subject: '',
    body_md: '',
  })
}

function moveStep(index: number, by: -1 | 1) {
  const target = index + by
  const list = steps.value
  const [moved] = list.splice(index, 1)
  if (moved) list.splice(target, 0, moved)
}

function removeStep(index: number) {
  steps.value.splice(index, 1)
}

function waitText(hours: number, first: boolean): string {
  const after = first ? 'after enrolling' : 'after the step before'
  if (hours === 0) return first ? 'Sent right away' : 'Sent with the step before'
  if (hours % 24 === 0) {
    const days = hours / 24
    return `${formatNumber(days)} ${days === 1 ? 'day' : 'days'} ${after}`
  }
  return `${formatNumber(hours)} ${hours === 1 ? 'hour' : 'hours'} ${after}`
}

const counts = computed(() => sequence.value?.enrollments)
const facts = computed(() => {
  const current = sequence.value
  if (!current) return []
  return [
    { key: 'list', label: 'List', value: list.value?.name ?? '' },
    { key: 'enrolling', label: 'Enrolling', value: current.enabled ? 'On' : 'Off' },
    { key: 'active', label: 'Active', value: formatNumber(counts.value?.active ?? 0) },
    { key: 'completed', label: 'Completed', value: formatNumber(counts.value?.completed ?? 0) },
    {
      key: 'left',
      label: 'Left',
      value: formatNumber((counts.value?.exited ?? 0) + (counts.value?.unsubscribed ?? 0)),
    },
  ]
})

const update = useUpdateSequence()
const remove = useDeleteSequence()
const enroll = useEnroll()
const exit = useExitEnrollment()

function save() {
  return runAction({
    run: () =>
      update.mutateAsync({
        path: { workspace_id: props.workspaceId, sequence_id: props.sequenceId },
        body: {
          name: name.value.trim(),
          trigger: trigger.value,
          enabled: enabled.value,
          steps: steps.value.map((step) => ({
            id: step.id ?? null,
            delay_hours: step.delay_hours,
            subject: step.subject.trim(),
            body_md: step.body_md,
          })),
        },
      }),
    success: 'Sequence saved',
  })
}

function onDelete() {
  return runAction({
    confirm: {
      title: `Delete “${sequence.value?.name ?? 'this sequence'}”?`,
      message:
        'Its steps and enrollments go away, and queued steps are not sent. The send log stays.',
      confirmLabel: 'Delete sequence',
    },
    run: async () => {
      await remove.mutateAsync({
        path: { workspace_id: props.workspaceId, sequence_id: props.sequenceId },
      })
      return true
    },
    success: 'Sequence deleted',
  }).then((deleted) => {
    if (deleted) {
      void router.push(`/w/${props.workspaceId}/p/${props.projectId}/mailing/sequences`)
    }
  })
}

const enrollOpen = ref(false)
const contactId = ref('')
const { data: subscribers } = useContacts(
  () => props.workspaceId,
  () => (enrollOpen.value ? (sequence.value?.list_id ?? null) : null),
  () => ({ status: 'subscribed' }),
)
const enrolled = computed(
  () => new Set((enrollments.value?.items ?? []).map((enrollment) => enrollment.contact_id)),
)
const contactOptions = computed(() =>
  (subscribers.value?.items ?? [])
    .filter((contact) => !enrolled.value.has(contact.id))
    .map((contact) => ({ value: contact.id, label: contact.email })),
)
watch(contactOptions, (options) => {
  if (enrollOpen.value && contactId.value === '' && options[0]) contactId.value = options[0].value
})

function openEnroll() {
  contactId.value = contactOptions.value[0]?.value ?? ''
  enroll.reset()
  enrollOpen.value = true
}

async function onEnroll() {
  await enroll.mutateAsync({
    path: { workspace_id: props.workspaceId, sequence_id: props.sequenceId },
    body: { contact_id: contactId.value },
  })
  enrollOpen.value = false
}

function onExit(enrollment: Enrollment) {
  return runAction({
    confirm: {
      title: `Take ${enrollment.email} out of the sequence?`,
      message: 'They get no more steps from it. They stay on the list.',
      confirmLabel: 'Take out',
    },
    run: () =>
      exit.mutateAsync({
        path: {
          workspace_id: props.workspaceId,
          sequence_id: props.sequenceId,
          enrollment_id: enrollment.id,
        },
      }),
    success: `${enrollment.email} left the sequence`,
  })
}

const actions = computed(() => [{ label: 'Enroll contact', icon: UserPlus, run: openEnroll }])
const totalSteps = computed(() => sequence.value?.steps.length ?? 0)
const enrollmentColumns = [
  { key: 'email', label: 'Email' },
  { key: 'status', label: 'Status' },
  { key: 'steps', label: 'Steps' },
  { key: 'next', label: 'Next step' },
  { key: 'enrolled', label: 'Enrolled' },
  { key: 'actions', label: 'Actions', hidden: true, class: 'w-16 text-right' },
]
const logColumns = [
  { key: 'to', label: 'To' },
  { key: 'subject', label: 'Subject' },
  { key: 'status', label: 'Status' },
  { key: 'sent', label: 'Sent' },
  { key: 'error', label: 'Note' },
]
</script>

<template>
  <Page>
    <ErrorAlert v-if="sequenceError" :error="sequenceError" />
    <template v-else-if="sequence">
      <PageHeader
        :section="projectName"
        :title="sequence.name"
        description="Timed mails each enrolled contact gets in order. Edits apply to steps a contact hasn't reached."
      >
        <DetailActions
          :actions="actions"
          :save="{ form: 'sequence-form', dirty, pending: update.asyncStatus.value === 'loading' }"
        >
          <template #menu>
            <MenuItem danger @select="onDelete">
              <Trash2 class="size-4" aria-hidden="true" />Delete sequence
            </MenuItem>
          </template>
        </DetailActions>
      </PageHeader>

      <DescriptionList :items="facts" />

      <form id="sequence-form" class="flex flex-col gap-6" @submit.prevent="save">
        <Card title="Settings">
          <div class="flex flex-col gap-4">
            <Field label="Name">
              <input v-model="name" name="sequence-name" required class="input w-full" />
            </Field>
            <Field label="Starts" hint="A change applies to contacts who subscribe after it.">
              <Select
                v-model="trigger"
                :options="triggerOptions"
                label="Starts"
                name="sequence-trigger"
              />
            </Field>
            <Checkbox v-model="enabled" label="Enroll new contacts" />
            <p class="text-sm text-base-content/70">
              Switching it off stops new enrollments. Contacts already in the sequence go on.
            </p>
          </div>
        </Card>

        <Card
          title="Steps"
          description="{{ name }} and {{ email }} fill in. Each mail has the list's footer and unsubscribe link."
        >
          <div class="flex flex-col gap-4">
            <p v-if="steps.length === 0" class="text-sm text-base-content/70">
              No steps yet. Add the first mail.
            </p>
            <fieldset
              v-for="(step, index) in steps"
              :key="step.key"
              class="flex flex-col gap-3 rounded-box border border-base-300 p-4"
              :aria-label="`Step ${index + 1}`"
            >
              <div class="flex items-center justify-between gap-2">
                <span class="text-sm font-medium">Step {{ index + 1 }}</span>
                <div class="flex gap-1">
                  <button
                    type="button"
                    class="btn btn-square btn-ghost btn-sm"
                    :aria-label="`Move step ${index + 1} up`"
                    :disabled="index === 0"
                    @click="moveStep(index, -1)"
                  >
                    <ArrowUp class="size-4" aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    class="btn btn-square btn-ghost btn-sm"
                    :aria-label="`Move step ${index + 1} down`"
                    :disabled="index === steps.length - 1"
                    @click="moveStep(index, 1)"
                  >
                    <ArrowDown class="size-4" aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    class="btn btn-square btn-ghost btn-sm"
                    :aria-label="`Remove step ${index + 1}`"
                    @click="removeStep(index)"
                  >
                    <Trash2 class="size-4" aria-hidden="true" />
                  </button>
                </div>
              </div>
              <Field label="Wait (hours)" :hint="waitText(step.delay_hours, index === 0)">
                <input
                  v-model.number="step.delay_hours"
                  type="number"
                  min="0"
                  max="8760"
                  required
                  :name="`step-${index}-delay`"
                  class="input w-32"
                />
              </Field>
              <Field label="Subject">
                <input
                  v-model="step.subject"
                  required
                  :name="`step-${index}-subject`"
                  class="input w-full"
                />
              </Field>
              <Field label="Body" hint="Markdown.">
                <textarea
                  v-model="step.body_md"
                  rows="6"
                  :name="`step-${index}-body`"
                  class="textarea w-full font-mono text-sm"
                ></textarea>
              </Field>
            </fieldset>
            <div>
              <button type="button" class="btn gap-1.5 btn-sm" @click="addStep">
                <Plus class="size-4" aria-hidden="true" />Add step
              </button>
            </div>
          </div>
        </Card>
      </form>

      <PageSection title="Enrollments">
        <template #description>The newest {{ ROWS_SHOWN }} contacts in this sequence.</template>
        <DataTable
          :columns="enrollmentColumns"
          :rows="enrollments?.items"
          :row-key="(e) => e.id"
          :loading="enrollmentsPending"
          :error="enrollmentsError"
          label="Enrollments"
          :empty="{
            icon: Users,
            title: 'Nobody enrolled yet',
            message:
              sequence.trigger === 'on_join'
                ? 'Contacts enroll when they subscribe while enrolling is on.'
                : 'Enroll subscribed contacts by hand or through the API.',
          }"
        >
          <template #cell-email="{ row }"
            ><span class="text-sm font-medium">{{ row.email }}</span></template
          >
          <template #cell-status="{ row }"><StatusBadge :status="row.status" /></template>
          <template #cell-steps="{ row }"
            ><span class="text-sm tabular-nums"
              >{{ formatNumber(row.steps_done) }} of {{ formatNumber(totalSteps) }}</span
            ></template
          >
          <template #cell-next="{ row }"
            ><span class="text-sm whitespace-nowrap">{{
              formatDateTime(row.next_run_at)
            }}</span></template
          >
          <template #cell-enrolled="{ row }"
            ><span class="text-sm whitespace-nowrap">{{
              formatDate(row.created_at)
            }}</span></template
          >
          <template #cell-actions="{ row }">
            <div v-if="row.status === 'active'" class="flex justify-end">
              <button
                type="button"
                class="btn btn-square btn-ghost btn-sm"
                :aria-label="`Take ${row.email} out`"
                @click="onExit(row)"
              >
                <LogOut class="size-4" aria-hidden="true" />
              </button>
            </div>
          </template>
        </DataTable>
      </PageSection>

      <PageSection title="Send log">
        <template #description>
          The newest {{ ROWS_SHOWN }} step mails. A script or a contact leaving can skip a step.
        </template>
        <DataTable
          :columns="logColumns"
          :rows="deliveries?.items"
          :row-key="(d) => d.id"
          :loading="deliveriesPending"
          :error="deliveriesError"
          label="Send log"
          :empty="{
            icon: Inbox,
            title: 'No mail yet',
            message: 'Each step mail shows here once it is queued.',
          }"
        >
          <template #cell-to="{ row }"
            ><span class="text-sm font-medium">{{ row.to_email }}</span></template
          >
          <template #cell-subject="{ row }"
            ><span class="text-sm">{{ row.subject }}</span></template
          >
          <template #cell-status="{ row }"><StatusBadge :status="row.status" /></template>
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

      <EntitySections
        :workspace-id="workspaceId"
        :project-id="projectId"
        kind="mailing.sequence"
        :entity-id="sequence.id"
      />
    </template>

    <FormPanel
      :open="enrollOpen"
      variant="dialog"
      title="Enroll a contact"
      description="Subscribed contacts of the list who aren't in this sequence. The first step follows its wait."
      submit-label="Enroll"
      :pending="enroll.asyncStatus.value === 'loading'"
      :error="enroll.error.value"
      :disabled="contactId === ''"
      @close="enrollOpen = false"
      @submit="onEnroll"
    >
      <p v-if="contactOptions.length === 0" class="text-sm text-base-content/80">
        Every subscribed contact is already enrolled, or the list has none.
      </p>
      <Field v-else label="Contact">
        <Select
          v-model="contactId"
          :options="contactOptions"
          label="Contact"
          name="enroll-contact"
        />
      </Field>
    </FormPanel>
  </Page>
</template>
