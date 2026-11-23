<script setup lang="ts">
import { Eye, RefreshCw, Settings, Trash2, Webhook } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import EntitySections from '@/components/EntitySections.vue'
import DeliveryBadge from '@/components/webhooks/DeliveryBadge.vue'
import RequestInspector from '@/components/webhooks/RequestInspector.vue'
import DetailActions from '@/components/ui/DetailActions.vue'
import Card from '@/components/ui/Card.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import CopyButton from '@/components/ui/CopyButton.vue'
import DataTable from '@/components/ui/DataTable.vue'
import Field from '@/components/ui/Field.vue'
import FormPanel from '@/components/ui/FormPanel.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'
import { runAction } from '@/lib/action'
import { formatDateTime } from '@/lib/format'
import {
  REQUESTS_SHOWN,
  useDeleteEndpoint,
  useEndpoint,
  useRequests,
  useUpdateEndpoint,
} from '@/queries/webhooks'

const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  endpointId: string
}>()
const router = useRouter()
const { data: endpoint, error: endpointError } = useEndpoint(
  () => props.workspaceId,
  () => props.endpointId,
)
// The URL's project comes from the link, the endpoint's from the server.
// Follow the server so the header, sidebar and back links match.
watch(endpoint, (loaded) => {
  if (loaded && loaded.project_id !== props.projectId) {
    void router.replace(`/w/${props.workspaceId}/p/${loaded.project_id}/webhooks/${loaded.id}`)
  }
})
const {
  data: requests,
  isPending,
  error,
  refetch,
  asyncStatus,
} = useRequests(
  () => props.workspaceId,
  () => props.endpointId,
)
const updateEndpoint = useUpdateEndpoint()
const deleteEndpoint = useDeleteEndpoint()
const viewing = ref<string | null>(null)

const columns = [
  { key: 'received', label: 'Received' },
  { key: 'method', label: 'Method' },
  { key: 'type', label: 'Content type' },
  { key: 'size', label: 'Size' },
  { key: 'delivery', label: 'Delivery' },
  { key: 'actions', label: 'Actions', hidden: true, class: 'w-16 text-right' },
]

const curl = computed(() =>
  endpoint.value
    ? `curl -X POST '${endpoint.value.url}' \\\n  -H 'content-type: application/json' \\\n  -d '{"hello": "world"}'`
    : '',
)

function onRefresh() {
  void refetch()
}

function onDelete() {
  const name = endpoint.value?.name ?? 'this endpoint'
  return runAction({
    confirm: {
      title: `Delete ${name}?`,
      message: 'This removes the endpoint and its requests. Senders get a 404.',
      confirmLabel: 'Delete endpoint',
    },
    run: async () => {
      await deleteEndpoint.mutateAsync({
        path: { workspace_id: props.workspaceId, endpoint_id: props.endpointId },
      })
      return true
    },
    success: `Deleted ${name}`,
  }).then(async (deleted) => {
    if (deleted) await router.push(`/w/${props.workspaceId}/p/${props.projectId}/webhooks`)
  })
}

const settingsOpen = ref(false)
const settings = ref({ name: '', enabled: true, forwardUrl: '' })

function openSettings() {
  const current = endpoint.value
  if (!current) return
  settings.value = {
    name: current.name,
    enabled: current.enabled,
    forwardUrl: current.forward_url ?? '',
  }
  updateEndpoint.reset()
  settingsOpen.value = true
}

async function onSaveSettings() {
  const s = settings.value
  const url = s.forwardUrl.trim()
  await updateEndpoint.mutateAsync({
    path: { workspace_id: props.workspaceId, endpoint_id: props.endpointId },
    body: { name: s.name, enabled: s.enabled, forward_url: url === '' ? null : url },
  })
  settingsOpen.value = false
}
</script>

<template>
  <Page>
    <PageHeader
      :section="projectName"
      :title="endpoint?.name ?? 'Endpoint'"
      :description="
        endpoint && !endpoint.enabled
          ? 'Disabled: requests get a 404 and nothing is stored.'
          : 'Requests sent to this endpoint’s URL, newest first.'
      "
    >
      <DetailActions
        :actions="[
          {
            label: 'Refresh',
            icon: RefreshCw,
            run: onRefresh,
            disabled: asyncStatus === 'loading',
          },
          { label: 'Settings', icon: Settings, run: openSettings },
        ]"
      >
        <template #menu>
          <MenuItem danger @select="onDelete">
            <Trash2 class="size-4" aria-hidden="true" />Delete endpoint
          </MenuItem>
        </template>
      </DetailActions>
    </PageHeader>

    <p v-if="endpointError" class="text-sm text-error">This endpoint doesn’t exist.</p>

    <Card v-if="endpoint" title="Endpoint">
      <div class="flex items-center gap-2">
        <code class="truncate text-sm" data-testid="endpoint-url">{{ endpoint.url }}</code>
        <CopyButton :text="endpoint.url" label="Copy URL" />
      </div>
      <p class="text-sm text-base-content/80">
        <template v-if="endpoint.forward_url">
          Forwards each request to <code class="text-xs">{{ endpoint.forward_url }}</code>.
        </template>
        <template v-else
          >Stores requests only. Set a forward URL in Settings to send them on.</template
        >
        Keeps the newest 500.
      </p>
      <details>
        <summary class="cursor-pointer text-sm font-medium">Send a test request</summary>
        <div class="mt-2 flex items-start gap-2">
          <pre tabindex="0" class="flex-1 overflow-x-auto rounded-field bg-base-200 p-3 text-xs">{{
            curl
          }}</pre>
          <CopyButton :text="curl" label="Copy curl command" />
        </div>
      </details>
    </Card>

    <PageSection title="Requests">
      <DataTable
        :columns="columns"
        :rows="requests?.items"
        :row-key="(r) => r.id"
        :loading="isPending"
        :error="error"
        label="Requests"
        :empty="{
          icon: Webhook,
          title: 'No requests yet',
          message: 'Send a request to the URL above, then refresh.',
        }"
      >
        <template #cell-received="{ row }"
          ><span class="text-sm whitespace-nowrap">{{
            formatDateTime(row.received_at)
          }}</span></template
        >
        <template #cell-method="{ row }">
          <span class="badge badge-ghost font-mono badge-sm">{{ row.method }}</span>
          <span v-if="row.query" class="ms-2 font-mono text-xs text-base-content/70"
            >?{{ row.query }}</span
          >
        </template>
        <template #cell-type="{ row }"
          ><span class="text-sm">{{ row.content_type ?? '—' }}</span></template
        >
        <template #cell-size="{ row }"
          ><span class="text-sm tabular-nums">{{ row.size }} B</span></template
        >
        <template #cell-delivery="{ row }">
          <DeliveryBadge v-if="row.delivery" :delivery="row.delivery" />
          <span v-else class="text-sm text-base-content/60">—</span>
        </template>
        <template #cell-actions="{ row }">
          <div class="flex justify-end">
            <button
              type="button"
              class="btn btn-square btn-ghost btn-sm"
              :aria-label="`Inspect the ${row.method} request from ${formatDateTime(row.received_at)}`"
              @click="viewing = row.id"
            >
              <Eye class="size-4" aria-hidden="true" />
            </button>
          </div>
        </template>
      </DataTable>
      <p v-if="requests?.next_before" class="text-sm text-base-content/80">
        Showing the newest {{ REQUESTS_SHOWN }}.
      </p>
    </PageSection>

    <EntitySections
      v-if="endpoint"
      :workspace-id="workspaceId"
      :project-id="projectId"
      kind="webhooks.endpoint"
      :entity-id="endpoint.id"
    />

    <RequestInspector
      :workspace-id="workspaceId"
      :endpoint-id="endpointId"
      :request-id="viewing"
      :forward-url="endpoint?.forward_url ?? null"
      @close="viewing = null"
    />

    <FormPanel
      :open="settingsOpen"
      title="Endpoint settings"
      submit-label="Save settings"
      :pending="updateEndpoint.asyncStatus.value === 'loading'"
      :error="updateEndpoint.error.value"
      @close="settingsOpen = false"
      @submit="onSaveSettings"
    >
      <Field label="Name">
        <input v-model="settings.name" name="settings-name" required class="input w-full" />
      </Field>
      <Checkbox v-model="settings.enabled" label="Accept requests" />
      <Field
        label="Forward to"
        hint="Optional. Each captured request is sent on to this public URL."
      >
        <input
          v-model="settings.forwardUrl"
          type="url"
          name="settings-forward-url"
          class="input w-full"
          placeholder="https://example.com/hooks"
        />
      </Field>
    </FormPanel>
  </Page>
</template>
