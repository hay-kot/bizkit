<script setup lang="ts">
import { KeyRound, Plus, TriangleAlert } from 'lucide-vue-next'
import { computed, ref } from 'vue'

import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import PageHeader from '@/components/PageHeader.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import CopyButton from '@/components/ui/CopyButton.vue'
import Field from '@/components/ui/Field.vue'
import Overlay from '@/components/ui/Overlay.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { confirmAction } from '@/lib/confirm'
import { errorMessage } from '@/lib/errors'
import { toast } from '@/lib/toast'
import type { ApiKey, CreatedApiKey } from '@/queries/apiKeys'
import { useApiKeys, useCreateApiKey, useRevokeApiKey } from '@/queries/apiKeys'
import { useMeta } from '@/queries/auth'

const props = defineProps<{ workspaceId: string; workspaceName: string }>()
const { data: keys, isPending, error } = useApiKeys(() => props.workspaceId)
const { data: meta } = useMeta()
const create = useCreateApiKey()
const revoke = useRevokeApiKey()

const levels = ['read', 'write', 'admin'] as const
const panelOpen = ref(false)
const name = ref('')
const scopes = ref<string[]>([])
const created = ref<CreatedApiKey>()
const areas = computed(() => meta.value?.scope_areas ?? [])

function openPanel() {
  name.value = ''
  scopes.value = []
  created.value = undefined
  create.reset()
  panelOpen.value = true
}

async function onCreate() {
  created.value = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { name: name.value, scopes: scopes.value },
  })
}

async function onRevoke(key: ApiKey) {
  const ok = await confirmAction({
    title: `Revoke ${key.name}?`,
    message: 'Anything using this key stops working right away.',
    confirmLabel: 'Revoke key',
  })
  if (!ok) return
  try {
    await revoke.mutateAsync({ path: { workspace_id: props.workspaceId, api_key_id: key.id } })
    toast(`Revoked ${key.name}`)
  } catch (err) {
    toast(errorMessage(err), 'error')
  }
}

function formatDate(value?: string | null) {
  return value
    ? new Date(value).toLocaleDateString(undefined, {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
      })
    : 'Never'
}
</script>

<template>
  <div>
    <PageHeader
      :section="workspaceName"
      title="API keys"
      description="Keys work in this workspace only, over the API and as MCP tools at /mcp."
    >
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New API key
      </button>
    </PageHeader>

    <SkeletonRows v-if="isPending" />
    <ErrorAlert v-else-if="error" :error="error" />
    <EmptyState
      v-else-if="!keys?.items.length"
      :icon="KeyRound"
      title="No API keys"
      message="Create a key for scripts, CI or an AI agent."
    >
      <button type="button" class="btn btn-primary btn-sm" @click="openPanel">New API key</button>
    </EmptyState>
    <div v-else class="overflow-x-auto rounded-box border border-base-300">
      <table class="table">
        <thead>
          <tr>
            <th scope="col">Name</th>
            <th scope="col">Key</th>
            <th scope="col">Scopes</th>
            <th scope="col">Last used</th>
            <th scope="col"><span class="sr-only">Actions</span></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="key in keys.items"
            :key="key.id"
            class="transition-colors hover:bg-base-200"
            :class="key.revoked_at ? 'opacity-70' : ''"
          >
            <td class="font-medium">{{ key.name }}</td>
            <td>
              <code class="text-sm">{{ key.prefix }}…</code>
            </td>
            <td>
              <div class="flex flex-wrap gap-1">
                <span
                  v-for="scope in key.scopes"
                  :key="scope"
                  class="badge badge-ghost font-mono badge-sm"
                  >{{ scope }}</span
                >
              </div>
            </td>
            <td class="text-sm">{{ formatDate(key.last_used_at) }}</td>
            <td class="text-right">
              <span v-if="key.revoked_at" class="badge badge-ghost badge-sm">revoked</span>
              <button
                v-else
                type="button"
                class="btn btn-ghost btn-sm"
                :aria-label="`Revoke ${key.name}`"
                @click="onRevoke(key)"
              >
                Revoke
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <Overlay
      :open="panelOpen"
      variant="panel"
      :title="created ? 'Key created' : 'New API key'"
      @close="panelOpen = false"
    >
      <div v-if="created" class="flex flex-col gap-4" data-testid="new-token">
        <div role="alert" class="alert alert-warning">
          <TriangleAlert class="size-4" aria-hidden="true" />
          <span>Copy this token now. You won't see it again.</span>
        </div>
        <div class="flex items-center gap-2 rounded-field border border-base-300 bg-base-200 p-2">
          <code class="flex-1 text-sm break-all select-all" data-testid="token-value">{{
            created.token
          }}</code>
          <CopyButton :text="created.token" label="Copy token" />
        </div>
        <p class="text-sm">Send it as <code>Authorization: Bearer &lt;token&gt;</code>.</p>
      </div>
      <form v-else id="new-key" class="flex flex-col gap-5" @submit.prevent="onCreate">
        <Field label="Name" hint="Something that tells you where the key is used.">
          <input v-model="name" name="key-name" required class="input w-full" placeholder="CI" />
        </Field>
        <fieldset class="flex flex-col gap-2">
          <legend class="mb-1 text-sm font-medium">Scopes</legend>
          <div class="rounded-box border border-base-300">
            <div
              v-for="area in areas"
              :key="area.name"
              class="flex flex-col gap-2 border-b border-base-300 px-3 py-2.5 last:border-b-0"
            >
              <div>
                <div class="text-sm font-medium">{{ area.name }}</div>
                <div class="text-xs text-base-content/70">{{ area.description }}</div>
              </div>
              <div class="flex gap-5">
                <Checkbox
                  v-for="level in levels"
                  :key="level"
                  v-model="scopes"
                  :value="`${area.name}:${level}`"
                  :label="level"
                />
              </div>
            </div>
          </div>
          <p class="text-xs text-base-content/70">Write includes read; admin includes both.</p>
        </fieldset>
        <ErrorAlert :error="create.error.value" />
      </form>
      <template #footer>
        <button
          v-if="created"
          type="button"
          class="btn btn-primary btn-sm"
          @click="panelOpen = false"
        >
          Done
        </button>
        <template v-else>
          <button type="button" class="btn btn-ghost btn-sm" @click="panelOpen = false">
            Cancel
          </button>
          <button
            type="submit"
            form="new-key"
            class="btn btn-primary btn-sm"
            :disabled="scopes.length === 0 || create.asyncStatus.value === 'loading'"
          >
            <span
              v-if="create.asyncStatus.value === 'loading'"
              class="loading loading-xs loading-spinner"
            ></span>
            Create key
          </button>
        </template>
      </template>
    </Overlay>
  </div>
</template>
