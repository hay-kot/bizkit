<script setup lang="ts">
import { CheckCircle2, FileSignature, LinkIcon, PenLine } from 'lucide-vue-next'
import { useDocumentVisibility, useIntervalFn } from '@vueuse/core'
import { computed, reactive, ref, watch } from 'vue'
import { useRoute } from 'vue-router'

import ErrorAlert from '@/components/ErrorAlert.vue'
import FieldBox from '@/components/signing/FieldBox.vue'
import PdfPages from '@/components/signing/PdfPages.vue'
import SignatureMark from '@/components/signing/SignatureMark.vue'
import SignaturePad from '@/components/signing/SignaturePad.vue'
import Card from '@/components/ui/Card.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import Spinner from '@/components/ui/Spinner.vue'
import SubmitButton from '@/components/ui/SubmitButton.vue'
import { errorCode } from '@/lib/errors'
import {
  documentUrl,
  isSignerSealing,
  MAX_TEXT,
  signedPdfUrl,
  useRecordView,
  useSign,
  useSignerSession,
  type SigningField,
  type SigningFieldFill,
  type SigningSignature,
} from '@/queries/signing'

type Mark = 'signature' | 'initials'

const route = useRoute()
const token = computed(() => (typeof route.query.t === 'string' ? route.query.t : ''))
const { data: session, error: loadError, isPending, pollGaveUp } = useSignerSession(token)
const recordView = useRecordView()
const sign = useSign()

const gone = computed(
  () =>
    token.value === '' ||
    errorCode(loadError.value) === 'not_found' ||
    errorCode(sign.error.value) === 'not_found',
)
const signed = computed(() => session.value?.signer.status === 'signed')
const fields = computed(() => session.value?.fields ?? [])
const title = computed(() => session.value?.request.title ?? '')

let viewRecorded = false
watch(
  session,
  (value) => {
    if (!value || viewRecorded || value.signer.status !== 'pending') return
    viewRecorded = true
    // A lost view only thins the audit trail, so a failure stays quiet.
    recordView.mutate({ body: { t: token.value } })
  },
  { immediate: true },
)

const adopted = reactive<Record<Mark, SigningSignature | undefined>>({
  signature: undefined,
  initials: undefined,
})
const texts = reactive<Record<string, string>>({})
const checks = reactive<Record<string, boolean>>({})
const consent = ref(false)

const needsInitials = computed(() => fields.value.some((f) => f.kind === 'initials'))
// The date fields get the server's UTC date at Sign; this is what it will
// be, kept fresh across midnight UTC and a tab left in the background.
const utcToday = () => new Date().toISOString().slice(0, 10)
const today = ref(utcToday())
useIntervalFn(() => (today.value = utcToday()), 60_000)
const visibility = useDocumentVisibility()
watch(visibility, (state) => {
  if (state === 'visible') today.value = utcToday()
})

const adopting = ref<Mark | null>(null)
const draft = ref<SigningSignature>()
const markLabel = { signature: 'signature', initials: 'initials' } as const

function openAdopt(mark: Mark) {
  draft.value = adopted[mark]
  adopting.value = mark
}

function adopt() {
  if (adopting.value && draft.value) adopted[adopting.value] = draft.value
  adopting.value = null
}

const initialsDefault = computed(() =>
  (session.value?.signer.name ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((part) => part[0]!.toUpperCase())
    .join(''),
)

function filled(field: SigningField): boolean {
  switch (field.kind) {
    case 'signature':
      return !!adopted.signature
    case 'initials':
      return !!adopted.initials
    case 'date':
      return true
    case 'text':
      return (texts[field.id] ?? '').trim() !== ''
    case 'checkbox':
      return !!checks[field.id]
  }
}

// A signer from before fields has none and still adopts a signature.
const left = computed(
  () =>
    fields.value.filter((field) => field.required && !filled(field)).length +
    (fields.value.length === 0 && !adopted.signature ? 1 : 0),
)
const ready = computed(() => left.value === 0 && consent.value)

const kindLabel: Record<SigningField['kind'], string> = {
  signature: 'Signature',
  initials: 'Initials',
  date: 'Date',
  text: 'Text',
  checkbox: 'Checkbox',
}

function describe(field: SigningField): string {
  const name = field.label ? `${kindLabel[field.kind]} "${field.label}"` : kindLabel[field.kind]
  return `Page ${field.page}, ${name}`
}

/** The fills in the order they're sent, so `values[i]` errors map back to fields. */
const fills = computed(() =>
  fields.value.flatMap((field): { field: SigningField; fill: SigningFieldFill }[] => {
    if (field.kind === 'text') {
      const text = (texts[field.id] ?? '').trim()
      return text ? [{ field, fill: { field_id: field.id, text } }] : []
    }
    if (field.kind === 'checkbox') {
      return [{ field, fill: { field_id: field.id, checked: !!checks[field.id] } }]
    }
    return []
  }),
)

const fieldNames: Record<string, string> = {
  consent: 'Consent',
  signature: 'Signature',
  initials: 'Initials',
}

/** The 422's problems, keyed by field ID, and the ones about the whole form. */
const problems = computed(() => {
  const byField: Record<string, string> = {}
  const general: string[] = []
  const error = sign.error.value as { error?: { fields?: Record<string, string[]> | null } }
  if (errorCode(error) !== 'validation') return { byField, general }
  for (const [path, messages] of Object.entries(error.error?.fields ?? {})) {
    const message = messages.join(', ')
    const direct = /^fields\.(.+)$/.exec(path)?.[1]
    const indexed = /^values\[(\d+)\]/.exec(path)?.[1]
    const field = direct
      ? fields.value.find((f) => f.id === direct)
      : indexed !== undefined
        ? fills.value[Number(indexed)]?.field
        : undefined
    if (field) {
      byField[field.id] = message
      general.push(`${describe(field)}: ${message}`)
    } else {
      general.push(`${fieldNames[path] ?? path}: ${message}`)
    }
  }
  return { byField, general }
})

const otherError = computed(() => {
  const code = errorCode(sign.error.value)
  return sign.error.value && code !== 'validation' && code !== 'not_found'
    ? sign.error.value
    : undefined
})

async function submit() {
  if (!ready.value || !adopted.signature) return
  await sign.mutateAsync({
    body: {
      t: token.value,
      consent: consent.value,
      signature: adopted.signature,
      initials: needsInitials.value ? adopted.initials : undefined,
      values: fills.value.map(({ fill }) => fill),
    },
  })
}

const done = computed(() => signed.value || sign.data.value !== undefined)
/** Signed here, and the session that says so hasn't come back yet. */
const refreshing = computed(() => sign.data.value !== undefined && !signed.value)
</script>

<template>
  <Page>
    <div v-if="gone" class="flex flex-col gap-3 rounded-box border border-base-300 bg-base-100 p-8">
      <span
        class="flex size-10 items-center justify-center rounded-full bg-base-200"
        aria-hidden="true"
      >
        <LinkIcon class="size-5" />
      </span>
      <h1 class="text-xl font-semibold">This link no longer works</h1>
      <p class="text-sm text-base-content/80">
        The request may have been cancelled or changed, or you signed it already. Ask the sender for
        a new link.
      </p>
    </div>

    <template v-else-if="isPending">
      <div class="h-16 w-1/2 animate-pulse rounded-box bg-base-300" aria-hidden="true"></div>
      <div
        class="aspect-3/4 w-full animate-pulse rounded-box bg-base-300"
        aria-busy="true"
        aria-label="Loading"
      ></div>
    </template>

    <ErrorAlert v-else-if="loadError && !session" :error="loadError" />

    <template v-else-if="session && done">
      <PageHeader :title="title" :icon="FileSignature" :crumbs="[]" />
      <p
        v-if="loadError"
        role="status"
        class="rounded-box border border-warning/50 bg-warning/10 px-3 py-2 text-sm"
      >
        Couldn't check for updates just now. This page tries again on its own.
      </p>
      <div
        class="flex flex-col items-start gap-3 rounded-box border border-base-300 bg-base-100 p-8"
        role="status"
      >
        <CheckCircle2 class="size-8 text-success" aria-hidden="true" />
        <h2 class="text-xl font-semibold">Thank you, you signed</h2>
        <template v-if="session.request.status === 'completed'">
          <p class="text-sm text-base-content/80">
            Everyone signed. The sealed PDF carries every signature and a certificate page.
          </p>
          <a class="btn btn-primary btn-sm" :href="signedPdfUrl(token)" download
            >Download the signed PDF</a
          >
        </template>
        <p
          v-else-if="refreshing || (isSignerSealing(session) && !pollGaveUp)"
          class="flex items-center gap-2 text-sm text-base-content/80"
        >
          <Spinner label="Waiting for the sealed PDF" />
          Sealing the document. The download appears here in a moment.
        </p>
        <p v-else-if="isSignerSealing(session)" class="text-sm text-base-content/80">
          Sealing is taking longer than usual. Reload this page later; the link keeps working.
        </p>
        <p
          v-else-if="session.request.status === 'sent'"
          class="text-sm text-base-content/80"
          data-testid="waiting-for-others"
        >
          Waiting for the others to sign. Once everyone has, we email you a link to the sealed PDF,
          and this page offers it too.
        </p>
        <p v-else class="text-sm text-base-content/80">
          The sealed copy is delayed. The sender can see it and try again.
        </p>
      </div>
    </template>

    <template v-else-if="session">
      <PageHeader
        :title="title"
        :icon="FileSignature"
        :crumbs="[]"
        :description="`For ${session.signer.name} (${session.signer.email}).`"
      />
      <p
        v-if="loadError"
        role="status"
        class="rounded-box border border-warning/50 bg-warning/10 px-3 py-2 text-sm"
      >
        Couldn't check for updates just now. Your fields are kept.
      </p>
      <p
        v-if="session.request.message"
        class="rounded-box border border-base-300 bg-base-100 p-4 text-sm whitespace-pre-line"
        data-testid="request-message"
      >
        {{ session.request.message }}
      </p>

      <PdfPages :url="documentUrl(token)" :title="session.request.file_name">
        <template #page="{ page }">
          <FieldBox
            v-for="field in fields.filter((f) => f.page === page)"
            :key="field.id"
            :rect="field"
          >
            <button
              v-if="field.kind === 'signature' || field.kind === 'initials'"
              type="button"
              class="flex size-full items-center justify-center overflow-hidden rounded-sm border-2 px-1 text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
              :class="[
                adopted[field.kind]
                  ? 'border-primary/40 bg-primary/5 text-base-content'
                  : 'border-dashed border-primary bg-primary/15 text-primary hover:bg-primary/25',
                problems.byField[field.id] ? 'border-error' : '',
              ]"
              :aria-label="`${describe(field)}: ${adopted[field.kind] ? 'change' : 'add'} your ${markLabel[field.kind]}`"
              @click="openAdopt(field.kind)"
            >
              <SignatureMark v-if="adopted[field.kind]" :mark="adopted[field.kind]!" />
              <span v-else class="inline-flex items-center gap-1 truncate">
                <PenLine class="size-3.5 shrink-0" aria-hidden="true" />
                {{ field.kind === 'signature' ? 'Sign' : 'Initial' }}
              </span>
            </button>
            <div
              v-else-if="field.kind === 'date'"
              class="flex size-full items-center overflow-hidden rounded-sm border border-base-300 bg-base-200/80 px-1 text-xs text-base-content/80 tabular-nums"
              :aria-label="`${describe(field)}: set to ${today} when you sign`"
              role="note"
            >
              {{ today }}
            </div>
            <input
              v-else-if="field.kind === 'text'"
              v-model="texts[field.id]"
              type="text"
              :maxlength="MAX_TEXT"
              :placeholder="field.label || 'Text'"
              :aria-label="describe(field)"
              :aria-required="field.required"
              :aria-invalid="!!problems.byField[field.id]"
              class="size-full rounded-sm border bg-field px-1 text-xs text-base-content placeholder:text-base-content/60 focus-visible:outline-2 focus-visible:outline-primary"
              :class="problems.byField[field.id] ? 'border-error' : 'border-primary'"
            />
            <div
              v-else
              class="flex size-full items-center justify-center rounded-sm border bg-field"
              :class="problems.byField[field.id] ? 'border-error' : 'border-primary'"
            >
              <Checkbox
                :model-value="checks[field.id] ?? false"
                @update:model-value="(value) => (checks[field.id] = value === true)"
              >
                <span class="sr-only">{{ describe(field) }}</span>
              </Checkbox>
            </div>
          </FieldBox>
        </template>
      </PdfPages>

      <Card
        title="Sign"
        :description="
          fields.length === 0
            ? 'Adopt your signature, then sign.'
            : 'Fill your fields on the document, then sign.'
        "
      >
        <form class="flex flex-col gap-4" @submit.prevent="submit">
          <div class="flex flex-wrap gap-4">
            <div
              v-for="mark in needsInitials
                ? (['signature', 'initials'] as const)
                : (['signature'] as const)"
              :key="mark"
              class="flex flex-col gap-1.5"
            >
              <span class="text-sm font-medium">{{
                mark === 'signature' ? 'Your signature' : 'Your initials'
              }}</span>
              <div class="flex items-center gap-3">
                <span
                  v-if="adopted[mark]"
                  class="flex h-12 w-40 items-center justify-center overflow-hidden rounded-field border border-base-300 bg-base-200 px-2"
                  :data-testid="`adopted-${mark}`"
                >
                  <SignatureMark :mark="adopted[mark]!" />
                </span>
                <button type="button" class="btn btn-sm" @click="openAdopt(mark)">
                  {{
                    adopted[mark]
                      ? 'Change'
                      : mark === 'signature'
                        ? 'Adopt signature'
                        : 'Adopt initials'
                  }}
                </button>
              </div>
            </div>
          </div>
          <Checkbox v-model="consent">{{ session.consent }}</Checkbox>
          <div v-if="problems.general.length > 0" role="alert" class="alert alert-error">
            <ul class="flex flex-col gap-1 text-sm">
              <li v-for="problem in problems.general" :key="problem">{{ problem }}</li>
            </ul>
          </div>
          <ErrorAlert :error="otherError" />
          <div class="flex flex-wrap items-center justify-end gap-3">
            <span v-if="left > 0" class="text-sm text-base-content/70">
              {{ left === 1 ? '1 field left' : `${left} fields left` }}
            </span>
            <span v-else-if="!consent" class="text-sm text-base-content/70">
              Tick the box to agree
            </span>
            <SubmitButton
              size="md"
              :pending="sign.asyncStatus.value === 'loading'"
              :disabled="!ready"
              >Sign</SubmitButton
            >
          </div>
        </form>
      </Card>

      <FormPanel
        variant="dialog"
        :open="adopting !== null"
        :title="adopting === 'initials' ? 'Adopt your initials' : 'Adopt your signature'"
        description="It goes in every field of yours that asks for it."
        submit-label="Adopt"
        :disabled="!draft"
        @close="adopting = null"
        @submit="adopt"
      >
        <SignaturePad
          v-if="adopting"
          :key="adopting"
          v-model="draft"
          :label="markLabel[adopting]"
          :typed-default="adopting === 'initials' ? initialsDefault : session.signer.name"
        />
      </FormPanel>
    </template>
  </Page>
</template>
