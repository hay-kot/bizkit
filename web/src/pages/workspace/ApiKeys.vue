<script setup lang="ts">
import { KeyRound, Plus } from 'lucide-vue-next'
import { computed, ref } from 'vue'

import ApiTokenPanel from '@/components/ApiTokenPanel.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import { runAction } from '@/lib/action'
import { formatDate } from '@/lib/format'
import type { ApiKey } from '@/queries/apiKeys'
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
const token = ref<string>()
const areas = computed(() => meta.value?.scope_areas ?? [])
const columns = [
  { key: 'name', label: 'Name' },
  { key: 'key', label: 'Key' },
  { key: 'scopes', label: 'Scopes' },
  { key: 'used', label: 'Last used' },
  { key: 'actions', label: 'Actions', hidden: true, class: 'text-right' },
]

function openPanel() {
  name.value = ''
  scopes.value = []
  create.reset()
  panelOpen.value = true
}

async function onCreate() {
  const created = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { name: name.value, scopes: scopes.value },
  })
  panelOpen.value = false
  token.value = created.token
}

function onRevoke(key: ApiKey) {
  return runAction({
    confirm: {
      title: `Revoke ${key.name}?`,
      message: 'Anything using this key stops working right away.',
      confirmLabel: 'Revoke key',
    },
    run: () =>
      revoke.mutateAsync({ path: { workspace_id: props.workspaceId, api_key_id: key.id } }),
    success: `Revoked ${key.name}`,
  })
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

    <DataTable
      :columns="columns"
      :rows="keys?.items"
      :row-key="(k) => k.id"
      :loading="isPending"
      :error="error"
      :row-class="(k) => (k.revoked_at ? 'opacity-70' : '')"
      :empty="{
        icon: KeyRound,
        title: 'No API keys',
        message: 'Create a key for scripts, CI or an AI agent.',
      }"
    >
      <template #cell-name="{ row }"
        ><span class="font-medium">{{ row.name }}</span></template
      >
      <template #cell-key="{ row }"
        ><code class="text-sm">{{ row.prefix }}…</code></template
      >
      <template #cell-scopes="{ row }">
        <div class="flex flex-wrap gap-1">
          <span
            v-for="scope in row.scopes"
            :key="scope"
            class="badge badge-ghost font-mono badge-sm"
            >{{ scope }}</span
          >
        </div>
      </template>
      <template #cell-used="{ row }"
        ><span class="text-sm">{{ formatDate(row.last_used_at, 'Never') }}</span></template
      >
      <template #cell-actions="{ row }">
        <span v-if="row.revoked_at" class="badge badge-ghost badge-sm">revoked</span>
        <button
          v-else
          type="button"
          class="btn btn-ghost btn-sm"
          :aria-label="`Revoke ${row.name}`"
          @click="onRevoke(row)"
        >
          Revoke
        </button>
      </template>
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">New API key</button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      title="New API key"
      submit-label="Create key"
      :pending="create.asyncStatus.value === 'loading'"
      :error="create.error.value"
      :disabled="scopes.length === 0"
      @close="panelOpen = false"
      @submit="onCreate"
    >
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
    </FormPanel>

    <ApiTokenPanel :token="token" @close="token = undefined" />
  </div>
</template>
