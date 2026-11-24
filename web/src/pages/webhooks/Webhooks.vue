<script setup lang="ts">
import { Plus, Webhook } from 'lucide-vue-next'
import { ref } from 'vue'
import { useRouter } from 'vue-router'

import EndpointUrl from '@/components/webhooks/EndpointUrl.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import { useCreateIntent } from '@/composables/useCreateIntent'
import { formatRelative } from '@/lib/format'
import { useCreateEndpoint, useEndpoints } from '@/queries/webhooks'

const props = defineProps<{ workspaceId: string; projectId: string; projectName: string }>()
const router = useRouter()
const {
  data: endpoints,
  isPending,
  error,
} = useEndpoints(
  () => props.workspaceId,
  () => props.projectId,
)
const create = useCreateEndpoint()
const panelOpen = ref(false)
useCreateIntent(openPanel)
const name = ref('')
const forwardUrl = ref('')
const columns = [
  { key: 'name', label: 'Name' },
  { key: 'url', label: 'URL' },
  { key: 'requests', label: 'Requests' },
  { key: 'last', label: 'Last request' },
]

function openPanel() {
  name.value = ''
  forwardUrl.value = ''
  create.reset()
  panelOpen.value = true
}

function endpointPath(endpointId: string) {
  return `/w/${props.workspaceId}/p/${props.projectId}/webhooks/${endpointId}`
}

async function onCreate() {
  const url = forwardUrl.value.trim()
  const endpoint = await create.mutateAsync({
    path: { workspace_id: props.workspaceId },
    body: { name: name.value, project_id: props.projectId, forward_url: url === '' ? null : url },
  })
  panelOpen.value = false
  await router.push(endpointPath(endpoint.id))
}
</script>

<template>
  <Page>
    <PageHeader
      title="Webhooks"
      description="Capture the webhooks other services send, inspect them, and send them on."
    >
      <button type="button" class="btn gap-1.5 btn-primary btn-sm" @click="openPanel">
        <Plus class="size-4" aria-hidden="true" />New endpoint
      </button>
    </PageHeader>

    <DataTable
      :columns="columns"
      :rows="endpoints?.items"
      :row-key="(e) => e.id"
      :loading="isPending"
      :error="error"
      :empty="{
        icon: Webhook,
        title: 'No endpoints yet',
        message: 'Create an endpoint, then give its URL to the service that sends webhooks.',
      }"
    >
      <template #cell-name="{ row }">
        <RouterLink :to="endpointPath(row.id)" class="link font-medium link-hover">
          {{ row.name }}
        </RouterLink>
        <span v-if="!row.enabled" class="ms-2 badge badge-ghost badge-sm">Disabled</span>
      </template>
      <template #cell-url="{ row }">
        <EndpointUrl :url="row.url" :name="row.name" />
      </template>
      <template #cell-requests="{ row }"
        ><span class="text-sm tabular-nums">{{ row.request_count }}</span></template
      >
      <template #cell-last="{ row }"
        ><span class="text-sm">{{ formatRelative(row.last_received_at, 'Never') }}</span></template
      >
      <template #empty-action>
        <button type="button" class="btn btn-primary btn-sm" @click="openPanel">
          New endpoint
        </button>
      </template>
    </DataTable>

    <FormPanel
      :open="panelOpen"
      title="New endpoint"
      submit-label="Create endpoint"
      :pending="create.asyncStatus.value === 'loading'"
      :error="create.error.value"
      @close="panelOpen = false"
      @submit="onCreate"
    >
      <Field label="Name">
        <input
          v-model="name"
          name="endpoint-name"
          required
          class="input w-full"
          placeholder="Stripe"
        />
      </Field>
      <Field
        label="Forward to"
        hint="Optional. Each captured request is sent on to this public URL."
      >
        <input
          v-model="forwardUrl"
          type="url"
          name="endpoint-forward-url"
          class="input w-full"
          placeholder="https://example.com/hooks"
        />
      </Field>
    </FormPanel>
  </Page>
</template>
