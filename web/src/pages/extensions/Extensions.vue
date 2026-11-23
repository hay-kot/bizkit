<script setup lang="ts">
import { Code, KeyRound, Plus, Trash2 } from 'lucide-vue-next'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'

import PageHeader from '@/components/PageHeader.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Select from '@/components/ui/Select.vue'
import { runAction } from '@/lib/action'
import { formatDate } from '@/lib/format'
import {
  useCreateScript,
  useDeleteSecret,
  useHooks,
  usePutSecret,
  useScripts,
  useSecrets,
} from '@/queries/extensions'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const router = useRouter()
const { data: hooks } = useHooks(() => props.workspaceId)
const {
  data: scripts,
  isPending,
  error,
} = useScripts(
  () => props.workspaceId,
  () => props.projectId,
)
const {
  data: secrets,
  isPending: secretsPending,
  error: secretsError,
} = useSecrets(
  () => props.workspaceId,
  () => props.projectId,
)
const create = useCreateScript()
const putSecret = usePutSecret()
const deleteSecret = useDeleteSecret()

const hookOptions = computed(() =>
  (hooks.value?.items ?? []).map((hook) => ({
    value: hook.id,
    label: hook.id,
    description: `${hook.kind === 'sync' ? 'In the request' : 'After the event'}: ${hook.summary}`,
  })),
)

const scriptColumns = [
  { key: 'name', label: 'Name' },
  { key: 'hook', label: 'Hook' },
  { key: 'updated', label: 'Updated' },
]
const secretColumns = [
  { key: 'name', label: 'Name' },
  { key: 'updated', label: 'Set' },
  { key: 'actions', label: 'Actions', hidden: true, class: 'w-16 text-right' },
]

const STARTERS: Record<string, string> = {
  sync: `// Return nothing to accept, or { action: 'reject', errors: { field: ['message'] } },
// or { action: 'spam', reason: 'why' }.
export default function (input, ctx) {
  ctx.log('checking', input.form.name)
}
`,
  async: `// Runs after the event. ctx.secrets holds this project's secrets; ctx.fetch reaches public URLs.
export default async function (input, ctx) {
  ctx.log('got', JSON.stringify(input))
}
`,
}

const panelOpen = ref(false)
const name = ref('')
const hook = ref('')

function openPanel() {
  name.value = ''
  hook.value = hookOptions.value[0]?.value ?? ''
  create.reset()
  panelOpen.value = true
}

function scriptPath(scriptId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/extensions/${scriptId}`
}

async function onCreate() {
  const point = hooks.value?.items.find((h) => h.id === hook.value)
  const script = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: {
      name: name.value,
      hook: hook.value,
      project_id: props.projectId,
      source: STARTERS[point?.kind ?? 'async'] ?? '',
    },
  })
  panelOpen.value = false
  await router.push(scriptPath(script.id))
}

const secretOpen = ref(false)
const secretName = ref('')
const secretValue = ref('')

function openSecret() {
  secretName.value = ''
  secretValue.value = ''
  putSecret.reset()
  secretOpen.value = true
}

async function onPutSecret() {
  await putSecret.mutateAsync({
    path: {
      workspace_id: props.workspaceId,
      project_id: props.projectId,
      name: secretName.value,
    },
    body: { value: secretValue.value },
  })
  secretOpen.value = false
}

function removeSecret(secret: string) {
  return runAction({
    confirm: {
      title: `Delete ${secret}?`,
      message: 'Scripts that read it get undefined from then on.',
      confirmLabel: 'Delete secret',
    },
    run: () =>
      deleteSecret.mutateAsync({
        path: { workspace_id: props.workspaceId, project_id: props.projectId, name: secret },
      }),
    success: 'Secret deleted',
  })
}
</script>

<template>
  <div>
    <PageHeader
      :section="projectName"
      title="Extensions"
      description="Scripts that run on this project's hook points, such as after each form submission."
    >
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New script
      </button>
    </PageHeader>

    <DataTable
      :columns="scriptColumns"
      :rows="scripts?.items"
      :row-key="(s) => s.id"
      :loading="isPending"
      :error="error"
      label="Scripts"
      :empty="{
        icon: Code,
        title: 'No scripts yet',
        message: 'Attach a script to a hook point to filter spam or post submissions elsewhere.',
      }"
    >
      <template #cell-name="{ row }">
        <RouterLink :to="scriptPath(row.id)" class="link font-medium link-hover">
          {{ row.name }}
        </RouterLink>
        <span v-if="!row.enabled" class="ms-2 badge badge-ghost badge-sm">Disabled</span>
        <span v-if="!row.available" class="ms-2 badge badge-sm badge-warning">Unavailable</span>
      </template>
      <template #cell-hook="{ row }">
        <code class="text-xs">{{ row.hook }}</code>
        <span class="ms-2 badge badge-outline badge-sm">{{ row.sync ? 'sync' : 'async' }}</span>
      </template>
      <template #cell-updated="{ row }"
        ><span class="text-sm">{{ formatDate(row.updated_at) }}</span></template
      >
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">New script</button>
      </template>
    </DataTable>

    <div class="mt-8 mb-3 flex items-center justify-between gap-2">
      <div>
        <h2 class="text-sm font-semibold">Secrets</h2>
        <p class="text-xs text-base-content/70">
          Async scripts read these as <code>ctx.secrets</code>. Values are never shown again, and
          run logs show <code>[secret:NAME]</code> instead.
        </p>
      </div>
      <button type="button" class="btn gap-1.5 btn-sm" @click="openSecret">
        <Plus class="size-4" aria-hidden="true" />Add secret
      </button>
    </div>

    <DataTable
      :columns="secretColumns"
      :rows="secrets?.items"
      :row-key="(s) => s.name"
      :loading="secretsPending"
      :error="secretsError"
      label="Secrets"
      :empty="{ icon: KeyRound, title: 'No secrets', message: 'Add a webhook URL or API token.' }"
    >
      <template #cell-name="{ row }"
        ><code class="text-sm">{{ row.name }}</code></template
      >
      <template #cell-updated="{ row }"
        ><span class="text-sm">{{ formatDate(row.updated_at) }}</span></template
      >
      <template #cell-actions="{ row }">
        <button
          type="button"
          class="btn btn-square btn-ghost btn-sm"
          :aria-label="`Delete ${row.name}`"
          @click="removeSecret(row.name)"
        >
          <Trash2 class="size-4" aria-hidden="true" />
        </button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      title="New script"
      submit-label="Create script"
      :pending="create.asyncStatus.value === 'loading'"
      :error="create.error.value"
      @close="panelOpen = false"
      @submit="onCreate"
    >
      <Field label="Name">
        <input
          v-model="name"
          name="script-name"
          required
          class="input w-full"
          placeholder="Post to Discord"
        />
      </Field>
      <Select v-model="hook" :options="hookOptions" label="Hook point" name="hook" />
    </FormPanel>

    <FormPanel
      :open="secretOpen"
      title="Add secret"
      submit-label="Save secret"
      :pending="putSecret.asyncStatus.value === 'loading'"
      :error="putSecret.error.value"
      @close="secretOpen = false"
      @submit="onPutSecret"
    >
      <Field label="Name" hint="Uppercase letters, digits and _, starting with a letter.">
        <input
          v-model="secretName"
          name="secret-name"
          required
          pattern="[A-Z][A-Z0-9_]{0,63}"
          class="input w-full font-mono"
          placeholder="DISCORD_WEBHOOK"
        />
      </Field>
      <Field label="Value" hint="Saving an existing name replaces its value.">
        <input
          v-model="secretValue"
          name="secret-value"
          type="password"
          required
          autocomplete="off"
          class="input w-full"
        />
      </Field>
    </FormPanel>
  </div>
</template>
