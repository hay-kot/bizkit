<script setup lang="ts">
import { Send } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'

import CodeEditor from '@/components/ui/CodeEditor.vue'
import CopyButton from '@/components/ui/CopyButton.vue'
import DetailPanel from '@/components/ui/DetailPanel.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import Field from '@/components/ui/Field.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import SubmitButton from '@/components/ui/SubmitButton.vue'
import { runAction } from '@/lib/action'
import { formatDateTime } from '@/lib/format'
import { useCapturedRequest, useReplayRequest } from '@/queries/webhooks'

import DeliveryBadge from './DeliveryBadge.vue'

/**
 * A side panel with one captured request in full: headers, body, the latest
 * delivery, and a form to replay it.
 */
const props = defineProps<{
  workspaceId: string
  endpointId: string
  requestId: string | null
  forwardUrl: string | null
}>()
const emit = defineEmits<{ close: [] }>()

const { data: request, error } = useCapturedRequest(
  () => props.workspaceId,
  () => props.endpointId,
  () => props.requestId,
)
const replay = useReplayRequest()
const replayUrl = ref('')
watch(
  () => props.requestId,
  () => {
    replayUrl.value = props.forwardUrl ?? ''
    replay.reset()
  },
)

const loaded = computed(() =>
  request.value && request.value.id === props.requestId ? request.value : undefined,
)

/** The body pretty-printed when it is JSON, else as sent. */
const body = computed(() => {
  const current = loaded.value
  if (!current) return { text: '', json: false }
  if (current.body_encoding === 'base64') return { text: current.body, json: false }
  try {
    return { text: JSON.stringify(JSON.parse(current.body), null, 2), json: true }
  } catch {
    return { text: current.body, json: false }
  }
})

function onReplay() {
  const current = loaded.value
  if (!current) return
  const url = replayUrl.value.trim()
  return runAction({
    run: () =>
      replay.mutateAsync({
        path: {
          workspace_id: props.workspaceId,
          endpoint_id: props.endpointId,
          request_id: current.id,
        },
        body: url === '' ? {} : { url },
      }),
    success: (delivery) =>
      delivery.error ? `Replay failed: ${delivery.error}` : `Replayed: ${delivery.status}`,
  })
}
</script>

<template>
  <DetailPanel
    :open="requestId !== null"
    :title="loaded ? `${loaded.method} request` : 'Request'"
    :description="loaded ? formatDateTime(loaded.received_at) : undefined"
    @close="emit('close')"
  >
    <ErrorAlert v-if="error" :error="error" />
    <SkeletonRows v-else-if="!loaded" :rows="6" />
    <div v-else class="flex flex-col gap-5 text-sm">
      <dl class="flex flex-col gap-2">
        <div v-if="loaded.query" class="flex flex-col gap-0.5">
          <dt class="font-medium">Query</dt>
          <dd class="font-mono text-xs break-all">?{{ loaded.query }}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
          <dt class="font-medium">Size</dt>
          <dd>{{ loaded.size }} bytes</dd>
        </div>
        <div v-if="loaded.delivery" class="flex flex-col gap-0.5">
          <dt class="font-medium">Latest delivery</dt>
          <dd class="flex flex-wrap items-center gap-2">
            <DeliveryBadge :delivery="loaded.delivery" />
            <span class="text-xs break-all">{{ loaded.delivery.url }}</span>
            <span class="text-xs text-base-content/70">
              {{ loaded.delivery.took_ms }} ms, {{ formatDateTime(loaded.delivery.at) }}
            </span>
          </dd>
          <dd v-if="loaded.delivery.error" class="text-xs text-error">
            {{ loaded.delivery.error }}
          </dd>
        </div>
      </dl>

      <section class="flex flex-col gap-2" aria-labelledby="request-headers">
        <h3 id="request-headers" class="font-medium">Headers</h3>
        <dl class="flex flex-col gap-1 rounded-field bg-base-200 p-3 font-mono text-xs">
          <div v-for="(header, i) in loaded.headers" :key="i" class="flex gap-2">
            <dt class="shrink-0 text-base-content/70">{{ header.name }}:</dt>
            <dd class="break-all">{{ header.value }}</dd>
          </div>
          <p v-if="loaded.headers.length === 0" class="text-base-content/70">No headers.</p>
        </dl>
      </section>

      <section class="flex flex-col gap-2" aria-labelledby="request-body">
        <div class="flex items-center justify-between gap-2">
          <h3 id="request-body" class="font-medium">
            Body
            <span v-if="loaded.body_encoding === 'base64'" class="badge badge-ghost badge-sm"
              >base64</span
            >
          </h3>
          <CopyButton v-if="body.text" :text="body.text" label="Copy body" />
        </div>
        <p v-if="!body.text" class="text-base-content/70">Empty.</p>
        <CodeEditor
          v-else-if="body.json"
          :model-value="body.text"
          label="Request body"
          language="json"
          readonly
          :max-lines="40"
        />
        <pre
          v-else
          tabindex="0"
          class="overflow-x-auto rounded-field bg-base-200 p-3 text-xs break-all whitespace-pre-wrap"
          >{{ body.text }}</pre>
      </section>

      <form class="flex flex-col gap-3" @submit.prevent="onReplay">
        <Field
          label="Replay to"
          :hint="
            forwardUrl
              ? 'Sends the stored method, headers and body again. Empty uses the forward URL.'
              : 'Sends the stored method, headers and body again. Public URLs only.'
          "
        >
          <input
            v-model="replayUrl"
            type="url"
            name="replay-url"
            :required="!forwardUrl"
            class="input w-full"
            placeholder="https://example.com/hooks"
          />
        </Field>
        <div class="flex justify-end">
          <SubmitButton :pending="replay.asyncStatus.value === 'loading'">
            <Send class="size-4" aria-hidden="true" />Replay
          </SubmitButton>
        </div>
      </form>
    </div>
  </DetailPanel>
</template>
