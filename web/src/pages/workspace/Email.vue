<script setup lang="ts">
import { Send, Trash2 } from 'lucide-vue-next'
import { computed, reactive, ref, watch } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import Card from '@/components/ui/Card.vue'
import CopyButton from '@/components/ui/CopyButton.vue'
import Field from '@/components/ui/Field.vue'
import Select from '@/components/ui/Select.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import SubmitButton from '@/components/ui/SubmitButton.vue'
import TabPage from '@/components/ui/TabPage.vue'
import { runAction } from '@/lib/action'
import { formatDateTime } from '@/lib/format'
import { toast } from '@/lib/toast'
import {
  type AccountProvider,
  type MailgunRegion,
  type MailSettings,
  type MailTestResult,
  type SmtpSecurity,
  useDeleteMailSettings,
  useMailSettings,
  useSaveMailSettings,
  useTestMailSettings,
} from '@/queries/mail'

const props = defineProps<{ workspaceId: string }>()
const { data, isPending, error } = useMailSettings(() => props.workspaceId)
const save = useSaveMailSettings()
const remove = useDeleteMailSettings()
const test = useTestMailSettings()

const providerOptions: { value: AccountProvider; label: string }[] = [
  { value: 'smtp', label: 'SMTP server' },
  { value: 'mailgun', label: 'Mailgun' },
]
const securityOptions: { value: SmtpSecurity; label: string; description: string }[] = [
  { value: 'starttls', label: 'STARTTLS', description: 'Usually port 587' },
  { value: 'tls', label: 'TLS', description: 'Usually port 465' },
  { value: 'none', label: 'None', description: 'Only for a local server in development' },
]
const regionOptions: { value: MailgunRegion; label: string }[] = [
  { value: 'us', label: 'US (api.mailgun.net)' },
  { value: 'eu', label: 'EU (api.eu.mailgun.net)' },
]

interface FormState {
  provider: AccountProvider
  fromName: string
  fromAddress: string
  replyTo: string
  host: string
  port: string
  security: SmtpSecurity
  username: string
  password: string
  domain: string
  region: MailgunRegion
  apiKey: string
  webhookKey: string
}

function fromSettings(settings: MailSettings | null | undefined): FormState {
  return {
    provider: settings?.provider ?? 'smtp',
    fromName: settings?.from_name ?? '',
    fromAddress: settings?.from_address ?? '',
    replyTo: settings?.reply_to ?? '',
    host: settings?.smtp?.host ?? '',
    port: String(settings?.smtp?.port ?? 587),
    security: settings?.smtp?.security ?? 'starttls',
    username: settings?.smtp?.username ?? '',
    password: '',
    domain: settings?.mailgun?.domain ?? '',
    region: settings?.mailgun?.region ?? 'us',
    apiKey: '',
    webhookKey: '',
  }
}

const form = reactive<FormState>(fromSettings(undefined))
const saved = ref(JSON.stringify(form))
watch(
  () => data.value?.settings,
  (settings) => {
    Object.assign(form, fromSettings(settings))
    saved.value = JSON.stringify(form)
  },
  { immediate: true },
)
const settings = computed(() => data.value?.settings ?? null)
const canSave = computed(() => data.value?.can_save ?? false)
const dirty = computed(() => JSON.stringify(form) !== saved.value)
const lastTest = ref<MailTestResult | null>(null)

function keptHint(saved: boolean | undefined) {
  return saved ? 'Saved. Leave it empty to keep it.' : undefined
}

function optional(text: string) {
  const trimmed = text.trim()
  return trimmed === '' ? undefined : trimmed
}

async function onSave() {
  const secret = (text: string) => (text === '' ? undefined : text)
  const result = await runAction({
    run: () =>
      save.mutateAsync({
        path: { workspace_id: props.workspaceId },
        body: {
          provider: form.provider,
          from_name: form.fromName,
          from_address: form.fromAddress,
          reply_to: optional(form.replyTo),
          smtp:
            form.provider === 'smtp'
              ? {
                  host: form.host,
                  port: Number(form.port),
                  security: form.security,
                  username: optional(form.username),
                  password: secret(form.password),
                }
              : undefined,
          mailgun:
            form.provider === 'mailgun'
              ? { domain: form.domain, region: form.region, api_key: secret(form.apiKey) }
              : undefined,
          webhook_signing_key: secret(form.webhookKey),
        },
      }),
    success: 'Mail settings saved',
  })
  if (result) lastTest.value = null
}

async function onTest() {
  lastTest.value = null
  const result = await runAction({
    run: () => test.mutateAsync({ path: { workspace_id: props.workspaceId } }),
  })
  if (!result) return
  lastTest.value = result
  if (result.sent) toast(`Test mail sent to ${result.to}`)
}

function onRemove() {
  return runAction({
    confirm: {
      title: 'Remove mail settings',
      message:
        "The instance mailer sends this workspace's mail again, from its own address. Mailgun events for this account stop arriving.",
      confirmLabel: 'Remove',
      danger: true,
    },
    run: () => remove.mutateAsync({ path: { workspace_id: props.workspaceId } }),
    success: 'Mail settings removed',
  })
}
</script>

<template>
  <TabPage
    title="Settings"
    description="Send this workspace's mail (mailing, form notifications, scripts) through your own mail account."
  >
    <ErrorAlert :error="error" />
    <SkeletonRows v-if="isPending" :rows="4" />
    <template v-else-if="data">
      <div v-if="!canSave" role="alert" class="alert alert-warning">
        <span>
          This instance has no secrets key (<code>secrets_key_env</code>), so workspaces can't save
          mail settings. Ask the person who runs bizkit to set one.
        </span>
      </div>
      <div
        v-else-if="settings && !settings.secrets_readable"
        role="alert"
        class="alert alert-warning"
      >
        <span>
          The saved passwords and keys don't open with this instance's secrets key. Mail from this
          workspace fails until you enter them again.
        </span>
      </div>

      <form @submit.prevent="onSave">
        <fieldset :disabled="!canSave" class="contents">
          <Card
            title="Sender and account"
            :description="
              settings
                ? 'Mail from this workspace goes out through this account.'
                : `Mail from this workspace goes out through the instance mailer, as ${data.instance_from}, until you set up an account.`
            "
          >
            <Field label="Provider" width="md">
              <Select v-model="form.provider" :options="providerOptions" label="Provider" />
            </Field>
            <Field label="From name" width="md">
              <input v-model="form.fromName" name="from-name" required class="input w-full" />
            </Field>
            <Field
              label="From address"
              width="md"
              hint="The account must be allowed to send from it."
            >
              <input
                v-model="form.fromAddress"
                name="from-address"
                type="email"
                required
                class="input w-full"
              />
            </Field>
            <Field
              label="Reply-To"
              width="md"
              hint="Optional. Where replies go, when not to the sender."
            >
              <input v-model="form.replyTo" name="reply-to" type="email" class="input w-full" />
            </Field>

            <template v-if="form.provider === 'smtp'">
              <Field label="Host" width="md">
                <input
                  v-model="form.host"
                  name="smtp-host"
                  required
                  placeholder="smtp.example.com"
                  class="input w-full"
                />
              </Field>
              <Field label="Port" width="md">
                <input
                  v-model="form.port"
                  name="smtp-port"
                  type="number"
                  min="1"
                  max="65535"
                  required
                  class="input w-full"
                />
              </Field>
              <Field label="Encryption" width="md">
                <Select v-model="form.security" :options="securityOptions" label="Encryption" />
              </Field>
              <Field
                label="User name"
                width="md"
                hint="Leave it empty when the server needs no sign-in."
              >
                <input
                  v-model="form.username"
                  name="smtp-username"
                  autocomplete="off"
                  class="input w-full"
                />
              </Field>
              <Field label="Password" width="md" :hint="keptHint(settings?.smtp?.has_password)">
                <input
                  v-model="form.password"
                  name="smtp-password"
                  type="password"
                  autocomplete="new-password"
                  class="input w-full"
                />
              </Field>
            </template>

            <template v-else>
              <Field label="Sending domain" width="md">
                <input
                  v-model="form.domain"
                  name="mailgun-domain"
                  required
                  placeholder="mg.example.com"
                  class="input w-full"
                />
              </Field>
              <Field label="Region" width="md">
                <Select v-model="form.region" :options="regionOptions" label="Region" />
              </Field>
              <Field label="API key" width="md" :hint="keptHint(settings?.mailgun?.has_api_key)">
                <input
                  v-model="form.apiKey"
                  name="mailgun-api-key"
                  type="password"
                  autocomplete="new-password"
                  class="input w-full"
                />
              </Field>
            </template>

            <Field
              label="Mailgun webhook signing key"
              width="md"
              :hint="
                keptHint(settings?.has_webhook_signing_key) ??
                'Optional. Lets Mailgun report bounces and complaints, so bizkit stops mailing those addresses.'
              "
            >
              <input
                v-model="form.webhookKey"
                name="webhook-signing-key"
                type="password"
                autocomplete="new-password"
                class="input w-full"
              />
            </Field>
            <Field
              v-if="settings?.webhook_url"
              label="Webhook URL"
              width="lg"
              hint="Point the account's event webhook (permanent failures and complaints) here."
            >
              <div class="flex items-center gap-2">
                <input
                  :value="settings.webhook_url"
                  readonly
                  aria-readonly="true"
                  class="input w-full bg-base-300/50 font-mono text-sm"
                />
                <CopyButton :text="settings.webhook_url" label="Copy webhook URL" />
              </div>
            </Field>

            <ErrorAlert :error="save.error.value" />
            <template #footer>
              <SubmitButton :pending="save.asyncStatus.value === 'loading'" :disabled="!dirty">
                Save changes
              </SubmitButton>
            </template>
          </Card>
        </fieldset>
      </form>

      <Card
        v-if="settings"
        title="Test"
        description="Send a test mail to yourself through the saved account."
      >
        <p class="text-sm">
          <template v-if="settings.verified_at">
            Verified {{ formatDateTime(settings.verified_at) }}.
          </template>
          <template v-else>Not verified since the last change.</template>
        </p>
        <div v-if="lastTest && !lastTest.sent" role="alert" class="alert alert-error">
          <span>The test mail to {{ lastTest.to }} failed: {{ lastTest.error }}</span>
        </div>
        <p v-else-if="lastTest?.sent" role="status" class="text-sm text-success">
          Sent to {{ lastTest.to }}. Check that inbox.
        </p>
        <template #footer>
          <button
            type="button"
            class="btn btn-sm"
            :disabled="dirty || test.asyncStatus.value === 'loading'"
            :aria-busy="test.asyncStatus.value === 'loading'"
            @click="onTest"
          >
            <Send class="size-4" aria-hidden="true" />Send test
          </button>
        </template>
      </Card>

      <Card
        v-if="settings"
        title="Danger zone"
        description="Remove this account. The instance mailer sends this workspace's mail again."
      >
        <template #footer>
          <button type="button" class="btn btn-ghost text-error btn-sm" @click="onRemove">
            <Trash2 class="size-4" aria-hidden="true" />Remove mail settings
          </button>
        </template>
      </Card>
    </template>
  </TabPage>
</template>
