<script setup lang="ts">
import { History, Play, Trash2 } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import CodeEditor from '@/components/extensions/CodeEditor.vue'
import PageHeader from '@/components/PageHeader.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import DataTable from '@/components/ui/DataTable.vue'
import DetailPanel from '@/components/ui/DetailPanel.vue'
import Field from '@/components/ui/Field.vue'
import SubmitButton from '@/components/ui/SubmitButton.vue'
import { runAction } from '@/lib/action'
import { errorMessage } from '@/lib/errors'
import { formatDate } from '@/lib/format'
import type { Run, RunStatus } from '@/queries/extensions'
import {
  useDeleteScript,
  useHooks,
  useRuns,
  useScript,
  useTestScript,
  useUpdateScript,
} from '@/queries/extensions'

const props = defineProps<{
  workspaceId: string
  projectId: string
  projectName: string
  scriptId: string
}>()
const router = useRouter()
const { data: script, error: scriptError } = useScript(
  () => props.workspaceId,
  () => props.scriptId,
)
const { data: hooks } = useHooks(() => props.workspaceId)
const {
  data: runs,
  isPending: runsPending,
  error: runsError,
  refetch: refetchRuns,
} = useRuns(
  () => props.workspaceId,
  () => props.scriptId,
)
const update = useUpdateScript()
const remove = useDeleteScript()
const test = useTestScript()

const hook = computed(() => hooks.value?.items.find((h) => h.id === script.value?.hook))
const name = ref('')
const source = ref('')
const enabled = ref(true)
const testInput = ref('')
const inputError = ref('')
const lastTest = ref<Run | null>(null)
const viewing = ref<Run | null>(null)

// The server copy the fields were last filled from. A refetch (after a
// save, failed or not) only replaces the fields the user hasn't edited since,
// so a rejected save or typing during a save never loses work.
let synced: typeof script.value
watch(
  script,
  (loaded) => {
    if (!loaded) return
    if (loaded.project_id !== props.projectId) {
      void router.replace(`/w/${props.workspaceId}/p/${loaded.project_id}/extensions/${loaded.id}`)
    }
    const fresh = synced?.id !== loaded.id
    if (fresh || name.value === synced?.name) name.value = loaded.name
    if (fresh || source.value === synced?.source) source.value = loaded.source
    if (fresh || enabled.value === synced?.enabled) enabled.value = loaded.enabled
    synced = loaded
  },
  { immediate: true },
)
watch(
  hook,
  (point) => {
    if (point && testInput.value === '') testInput.value = JSON.stringify(point.sample, null, 2)
  },
  { immediate: true },
)

const dirty = computed(
  () =>
    script.value !== undefined &&
    (name.value !== script.value.name ||
      source.value !== script.value.source ||
      enabled.value !== script.value.enabled),
)

const STATUS_LABELS: Record<RunStatus, string> = {
  ok: 'OK',
  error: 'Error',
  timeout: 'Timed out',
  overloaded: 'Busy',
  invalid_output: 'Invalid output',
}
const STATUS_CLASSES: Record<RunStatus, string> = {
  ok: 'badge-success',
  error: 'badge-error',
  timeout: 'badge-warning',
  overloaded: 'badge-warning',
  invalid_output: 'badge-error',
}

const columns = [
  { key: 'when', label: 'When' },
  { key: 'cause', label: 'Cause' },
  { key: 'status', label: 'Status' },
  { key: 'duration', label: 'Took' },
  { key: 'summary', label: 'Details' },
]

function save() {
  return runAction({
    run: () =>
      update.mutateAsync({
        path: { workspace_id: props.workspaceId, script_id: props.scriptId },
        body: { name: name.value, source: source.value, enabled: enabled.value },
      }),
    success: 'Script saved',
  })
}

type Json = string | number | boolean | unknown[] | { [key: string]: unknown } | null

function parsedInput(): Json | undefined {
  inputError.value = ''
  try {
    return JSON.parse(testInput.value) as Json
  } catch {
    inputError.value = 'The input must be valid JSON.'
    return undefined
  }
}

async function runTest() {
  const input = parsedInput()
  if (input === undefined) return
  lastTest.value = await test.mutateAsync({
    path: { workspace_id: props.workspaceId, script_id: props.scriptId },
    body: { input, source: source.value },
  })
  await refetchRuns()
}

function deleteScript() {
  return runAction({
    confirm: {
      title: 'Delete this script?',
      message: 'Its run log goes too. You can’t undo this.',
      confirmLabel: 'Delete script',
    },
    run: () =>
      remove.mutateAsync({ path: { workspace_id: props.workspaceId, script_id: props.scriptId } }),
    success: 'Script deleted',
  }).then(() => router.push(`/w/${props.workspaceId}/p/${props.projectId}/extensions`))
}

function summaryOf(run: Run): string {
  return run.error ?? run.logs.at(-1) ?? (run.output === null ? '' : JSON.stringify(run.output))
}
</script>

<template>
  <div>
    <p v-if="scriptError" class="text-error">{{ errorMessage(scriptError) }}</p>
    <template v-else-if="script">
      <PageHeader
        :section="projectName"
        :title="script.name"
        :description="hook?.summary ?? 'This hook point isn’t compiled into this server.'"
      >
        <button
          type="button"
          class="btn gap-1.5 btn-ghost btn-sm"
          :disabled="remove.asyncStatus.value === 'loading'"
          @click="deleteScript"
        >
          <Trash2 class="size-4" aria-hidden="true" />Delete
        </button>
      </PageHeader>

      <div class="mb-4 flex flex-wrap items-center gap-2 text-sm">
        <code>{{ script.hook }}</code>
        <span class="badge badge-outline badge-sm">{{ script.sync ? 'sync' : 'async' }}</span>
        <span v-if="hook" class="text-base-content/70">
          Budget {{ hook.budget_ms }} ms · ctx: {{ hook.capabilities.join(', ') }}
          <template v-if="!script.sync">
            · Failed runs retry, 3 attempts in all, so repeats must be harmless
          </template>
        </span>
      </div>

      <form class="mb-8 flex flex-col gap-4" @submit.prevent="save">
        <div class="flex flex-wrap items-end gap-4">
          <Field label="Name" class="min-w-64 flex-1">
            <input v-model="name" name="script-name" required class="input w-full" />
          </Field>
          <Checkbox v-model="enabled" label="Enabled" />
        </div>
        <CodeEditor v-model="source" name="source" label="Script source" />
        <div class="flex items-center gap-2">
          <SubmitButton :pending="update.asyncStatus.value === 'loading'" :disabled="!dirty">
            Save
          </SubmitButton>
          <span v-if="dirty" class="text-xs text-base-content/70">Unsaved changes</span>
        </div>
      </form>

      <section class="mb-8" aria-labelledby="test-heading">
        <h2 id="test-heading" class="mb-1 text-sm font-semibold">Test run</h2>
        <p class="mb-2 text-xs text-base-content/70">
          Runs the source above, saved or not, with this input.
          <template v-if="!script.sync">Fetches are real.</template>
        </p>
        <CodeEditor v-model="testInput" name="test-input" label="Test input" :rows="8" />
        <p v-if="inputError" class="mt-1 text-xs text-error">{{ inputError }}</p>
        <button
          type="button"
          class="btn mt-2 gap-1.5 btn-sm"
          :disabled="test.asyncStatus.value === 'loading'"
          @click="runTest"
        >
          <Play class="size-4" aria-hidden="true" />Run test
        </button>
        <p v-if="test.error.value" class="mt-2 text-sm text-error">
          {{ errorMessage(test.error.value) }}
        </p>
        <div
          v-if="lastTest"
          class="mt-3 rounded-box border border-base-300 p-3"
          data-testid="test-result"
        >
          <div class="mb-2 flex items-center gap-2 text-sm">
            <span class="badge badge-sm" :class="STATUS_CLASSES[lastTest.status]">
              {{ STATUS_LABELS[lastTest.status] }}
            </span>
            <span class="text-base-content/70">{{ lastTest.duration_ms }} ms</span>
          </div>
          <p v-if="lastTest.error" class="mb-2 text-sm text-error">{{ lastTest.error }}</p>
          <pre
            v-if="lastTest.logs.length"
            class="mb-2 overflow-x-auto rounded-field bg-base-200 p-2 text-xs"
            >{{ lastTest.logs.join('\n') }}</pre>
          <pre
            v-if="lastTest.output !== null"
            class="overflow-x-auto rounded-field bg-base-200 p-2 text-xs"
            >{{ JSON.stringify(lastTest.output, null, 2) }}</pre>
        </div>
      </section>

      <h2 class="mb-3 text-sm font-semibold">Run log</h2>
      <DataTable
        :columns="columns"
        :rows="runs?.items"
        :row-key="(r) => r.id"
        :loading="runsPending"
        :error="runsError"
        label="Runs"
        :empty="{
          icon: History,
          title: 'No runs yet',
          message: script.sync
            ? 'Runs show here as submissions come in.'
            : 'Runs show here after each event.',
        }"
      >
        <template #cell-when="{ row }">
          <button
            type="button"
            class="link text-sm whitespace-nowrap link-hover"
            @click="viewing = row"
          >
            {{ formatDate(row.created_at) }}
          </button>
        </template>
        <template #cell-cause="{ row }">
          <span class="text-sm"
            >{{ row.cause }}<template v-if="row.attempt"> #{{ row.attempt }}</template></span
          >
        </template>
        <template #cell-status="{ row }">
          <span class="badge badge-sm" :class="STATUS_CLASSES[row.status]">
            {{ STATUS_LABELS[row.status] }}
          </span>
        </template>
        <template #cell-duration="{ row }"
          ><span class="text-sm tabular-nums">{{ row.duration_ms }} ms</span></template
        >
        <template #cell-summary="{ row }"
          ><span class="line-clamp-1 text-sm text-base-content/80">{{
            summaryOf(row)
          }}</span></template
        >
      </DataTable>

      <DetailPanel
        :open="viewing !== null"
        :title="viewing ? `Run ${formatDate(viewing.created_at)}` : ''"
        @close="viewing = null"
      >
        <template v-if="viewing">
          <p class="mb-3 text-sm">
            <span class="badge badge-sm" :class="STATUS_CLASSES[viewing.status]">
              {{ STATUS_LABELS[viewing.status] }}
            </span>
            {{ viewing.cause }} · {{ viewing.duration_ms }} ms
          </p>
          <p v-if="viewing.error" class="mb-3 text-sm text-error">{{ viewing.error }}</p>
          <h3 class="mb-1 text-xs font-semibold">Logs</h3>
          <pre class="mb-3 overflow-x-auto rounded-field bg-base-200 p-2 text-xs">{{
            viewing.logs.length ? viewing.logs.join('\n') : '(none)'
          }}</pre>
          <h3 class="mb-1 text-xs font-semibold">Output</h3>
          <pre class="overflow-x-auto rounded-field bg-base-200 p-2 text-xs">{{
            viewing.output === null ? '(none)' : JSON.stringify(viewing.output, null, 2)
          }}</pre>
        </template>
      </DetailPanel>
    </template>
  </div>
</template>
