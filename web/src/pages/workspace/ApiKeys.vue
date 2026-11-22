<script setup lang="ts">
import { computed, ref } from 'vue'

import EmptyState from '@/components/EmptyState.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import Loading from '@/components/Loading.vue'
import type { CreatedApiKey } from '@/queries/apiKeys'
import { useApiKeys, useCreateApiKey, useRevokeApiKey } from '@/queries/apiKeys'
import { useMeta } from '@/queries/auth'

const props = defineProps<{ workspaceId: string }>()
const { data: keys, isPending, error } = useApiKeys(() => props.workspaceId)
const { data: meta } = useMeta()
const create = useCreateApiKey()
const revoke = useRevokeApiKey()

const levels = ['read', 'write', 'admin'] as const
const name = ref('')
const scopes = ref<string[]>([])
const created = ref<CreatedApiKey>()
const areas = computed(() => meta.value?.scope_areas ?? [])

async function onCreate() {
  created.value = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { name: name.value, scopes: scopes.value },
  })
  name.value = ''
  scopes.value = []
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <div
      v-if="created"
      role="status"
      class="alert flex-col items-start alert-success"
      data-testid="new-token"
    >
      <span class="font-medium">Copy this token now. You won't see it again.</span>
      <code class="break-all select-all">{{ created.token }}</code>
    </div>
    <section class="card bg-base-100" aria-labelledby="new-key-heading">
      <form class="card-body gap-4" @submit.prevent="onCreate">
        <h2 id="new-key-heading" class="card-title text-lg">New API key</h2>
        <label class="flex flex-col gap-1">
          <span class="text-sm font-medium">Name</span>
          <input v-model="name" name="key-name" required class="input w-full" placeholder="CI" />
        </label>
        <fieldset class="fieldset">
          <legend class="fieldset-legend">Scopes</legend>
          <div
            v-for="area in areas"
            :key="area.name"
            class="flex flex-wrap items-center gap-4 py-1"
          >
            <span class="w-28 text-sm font-medium" :title="area.description">{{ area.name }}</span>
            <label v-for="level in levels" :key="level" class="flex items-center gap-2 text-sm">
              <input
                v-model="scopes"
                type="checkbox"
                class="checkbox checkbox-sm"
                :value="`${area.name}:${level}`"
              />
              {{ level }}
            </label>
          </div>
        </fieldset>
        <ErrorAlert :error="create.error.value" />
        <div class="card-actions justify-end">
          <button type="submit" class="btn btn-primary" :disabled="scopes.length === 0">
            Create key
          </button>
        </div>
      </form>
    </section>
    <section class="card bg-base-100" aria-labelledby="keys-heading">
      <div class="card-body">
        <h2 id="keys-heading" class="card-title text-lg">Keys</h2>
        <Loading v-if="isPending" />
        <ErrorAlert v-else-if="error" :error="error" />
        <EmptyState v-else-if="!keys?.items.length" message="No API keys yet." />
        <div v-else class="overflow-x-auto">
          <table class="table">
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Key</th>
                <th scope="col">Scopes</th>
                <th scope="col"><span class="sr-only">Actions</span></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="key in keys.items" :key="key.id">
                <td class="font-medium">{{ key.name }}</td>
                <td>
                  <code>{{ key.prefix }}…</code>
                </td>
                <td class="text-sm">{{ key.scopes.join(', ') }}</td>
                <td class="text-right">
                  <span v-if="key.revoked_at" class="badge badge-ghost">revoked</span>
                  <button
                    v-else
                    type="button"
                    class="btn btn-ghost btn-xs"
                    :aria-label="`Revoke ${key.name}`"
                    @click="
                      revoke.mutate({ path: { workspace_id: workspaceId, api_key_id: key.id } })
                    "
                  >
                    Revoke
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </section>
  </div>
</template>
