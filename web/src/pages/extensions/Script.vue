<script setup lang="ts">
import { CircleCheck, CircleDot, Ellipsis, History, Play, Trash2 } from 'lucide-vue-next'
import { computed, ref, useTemplateRef, watch } from 'vue'
import { useRouter } from 'vue-router'

import PageHeader from '@/components/PageHeader.vue'
import Card from '@/components/ui/Card.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import CodeEditor from '@/components/ui/CodeEditor.vue'
import DataTable from '@/components/ui/DataTable.vue'
import DescriptionList from '@/components/ui/DescriptionList.vue'
import DetailPanel from '@/components/ui/DetailPanel.vue'
import Field from '@/components/ui/Field.vue'
import JsonEditor from '@/components/ui/JsonEditor.vue'
import Menu from '@/components/ui/Menu.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import SectionHeading from '@/components/ui/SectionHeading.vue'
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
const inputEditor = useTemplateRef('inputEditor')
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

const CAPABILITY_NAMES: Record<string, string> = {
  log: 'ctx.log',
  secrets: 'ctx.secrets',
  fetch: 'ctx.fetch',
}

/** What the script runs for, in words. */
const runsFor = computed(() => {
  const kind = hook.value?.target
  const target = script.value?.target
  if (!target) return kind ? `All ${kind.plural}` : 'The project'
  return target.label ?? `A deleted ${kind?.noun ?? 'entity'}`
})

const facts = computed(() => {
  const point = hook.value
  const items = [
    { key: 'hook', label: 'Hook point' },
    { key: 'target', label: 'Runs for', value: runsFor.value },
    {
      key: 'mode',
      label: 'Runs',
      value: script.value?.sync ? 'In the request (sync)' : 'After each event (async)',
    },
    { key: 'budget', label: 'Time budget', value: point ? `${point.budget_ms} ms` : 'Unknown' },
    {
      key: 'capabilities',
      label: 'Can use',
      value: point ? point.capabilities.map((c) => CAPABILITY_NAMES[c] ?? c).join(', ') : '',
    },
  ]
  return items
})

const sampleInput = computed(() => (hook.value ? JSON.stringify(hook.value.sample, null, 2) : ''))

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

const testOutput = computed(() =>
  lastTest.value?.output == null ? '' : JSON.stringify(lastTest.value.output, null, 2),
)

async function runTest() {
  const parsed = inputEditor.value?.parsed
  if (!parsed?.ok) return
  lastTest.value = await test.mutateAsync({
    path: { workspace_id: props.workspaceId, script_id: props.scriptId },
    body: { input: parsed.value, source: source.value },
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
        :description="
          hook
            ? `Runs ${script.sync ? 'on' : 'after'} ${script.hook} for ${script.target ? runsFor : runsFor.toLowerCase()}.`
            : 'This hook point isn’t compiled into this server, so the script never runs.'
        "
      >
        <span
          class="flex items-center gap-1.5 text-xs"
          :class="dirty ? 'text-warning' : 'text-base-content/60'"
          role="status"
        >
          <component :is="dirty ? CircleDot : CircleCheck" class="size-3.5" aria-hidden="true" />
          {{ dirty ? 'Unsaved changes' : 'Saved' }}
        </span>
        <SubmitButton
          form="script-form"
          :pending="update.asyncStatus.value === 'loading'"
          :disabled="!dirty"
        >
          Save
        </SubmitButton>
        <Menu label="Script actions" align="end">
          <template #trigger="{ open, toggle, controls }">
            <button
              type="button"
              class="btn btn-square btn-ghost btn-sm"
              aria-label="More actions"
              aria-haspopup="menu"
              :aria-expanded="open"
              :aria-controls="controls"
              @click="toggle"
            >
              <Ellipsis class="size-4" aria-hidden="true" />
            </button>
          </template>
          <MenuItem @select="deleteScript">
            <Trash2 class="size-4 text-error" aria-hidden="true" />Delete script
          </MenuItem>
        </Menu>
      </PageHeader>

      <DescriptionList :items="facts" class="mb-6">
        <template #hook>
          <code class="text-xs">{{ script.hook }}</code>
        </template>
      </DescriptionList>

      <div class="grid items-start gap-6 xl:grid-cols-5">
        <Card title="Script" class="xl:col-span-3">
          <form id="script-form" class="flex flex-col gap-4" @submit.prevent="save">
            <div class="flex flex-wrap items-end gap-4">
              <Field label="Name" class="min-w-64 flex-1">
                <input v-model="name" name="script-name" required class="input w-full" />
              </Field>
              <div class="flex h-10 items-center">
                <Checkbox v-model="enabled" label="Enabled" />
              </div>
            </div>
            <div class="flex flex-col gap-1.5">
              <span class="text-sm font-medium">Source</span>
              <CodeEditor
                v-model="source"
                language="javascript"
                label="Script source"
                :min-lines="12"
                :max-lines="36"
              />
            </div>
          </form>
          <details v-if="hook" class="rounded-field border border-base-300 px-3 py-2">
            <summary class="cursor-pointer text-sm font-medium">
              What the script gets and returns
            </summary>
            <p class="mt-2 text-sm text-base-content/80">{{ hook.summary }}</p>
            <p v-if="!script.sync" class="mt-2 text-sm text-base-content/80">
              A failed run retries, 3 attempts in all, and each attempt runs the whole script, so
              repeated runs must be harmless.
            </p>
            <p class="mt-3 mb-1 text-xs font-medium">Sample input</p>
            <pre tabindex="0" class="overflow-x-auto rounded-field bg-base-200 p-2 text-xs">{{
              sampleInput
            }}</pre>
          </details>
        </Card>

        <Card
          title="Test run"
          class="xl:col-span-2"
          :description="`Runs the source on the left, saved or not.${script.sync ? '' : ' Fetches are real.'}`"
        >
          <template #actions>
            <button
              type="button"
              class="btn gap-1.5 btn-sm"
              :disabled="test.asyncStatus.value === 'loading' || inputEditor?.parsed.ok === false"
              @click="runTest"
            >
              <Play class="size-4" aria-hidden="true" />Run test
            </button>
          </template>
          <div class="flex flex-col gap-1.5">
            <span class="text-sm font-medium">Input</span>
            <JsonEditor ref="inputEditor" v-model="testInput" label="Test input" :max-lines="16" />
          </div>
          <div class="flex flex-col gap-1.5">
            <span class="text-sm font-medium">Result</span>
            <p v-if="test.error.value" class="text-sm text-error">
              {{ errorMessage(test.error.value) }}
            </p>
            <div
              v-if="lastTest"
              class="flex flex-col gap-3 rounded-field border border-base-300 p-3"
              data-testid="test-result"
            >
              <div class="flex items-center gap-2 text-sm">
                <span class="badge badge-sm" :class="STATUS_CLASSES[lastTest.status]">
                  {{ STATUS_LABELS[lastTest.status] }}
                </span>
                <span class="text-base-content/70 tabular-nums">{{ lastTest.duration_ms }} ms</span>
              </div>
              <p v-if="lastTest.error" class="text-sm text-error">{{ lastTest.error }}</p>
              <div>
                <p class="mb-1 text-xs font-medium">Logs</p>
                <pre
                  v-if="lastTest.logs.length"
                  tabindex="0"
                  class="overflow-x-auto rounded-field bg-base-200 p-2 text-xs"
                  >{{ lastTest.logs.join('\n') }}</pre>
                <p v-else class="text-xs text-base-content/70">Nothing logged.</p>
              </div>
              <div>
                <p class="mb-1 text-xs font-medium">Returned</p>
                <CodeEditor
                  v-if="lastTest.output !== null"
                  :model-value="testOutput"
                  language="json"
                  label="Test output"
                  readonly
                  :min-lines="1"
                  :max-lines="16"
                />
                <p v-else class="text-xs text-base-content/70">Nothing (undefined).</p>
              </div>
            </div>
            <p
              v-else
              class="rounded-field border border-dashed border-base-300 p-4 text-center text-sm text-base-content/70"
            >
              Run the script to see its status, logs and return value here.
            </p>
          </div>
        </Card>
      </div>

      <SectionHeading
        title="Run log"
        description="The newest 100 runs, test runs included."
        class="mt-8 mb-3"
      />
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
