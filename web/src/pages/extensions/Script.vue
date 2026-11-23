<script setup lang="ts">
import { FlaskConical, History, Play, Trash2 } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import ApiReference from '@/components/extensions/ApiReference.vue'
import TargetPicker from '@/components/extensions/TargetPicker.vue'
import { runsForText } from '@/components/extensions/targets'
import type { CasesResult } from '@/components/extensions/testCases'
import { lineDiff, parseCases } from '@/components/extensions/testCases'
import Card from '@/components/ui/Card.vue'
import Checkbox from '@/components/ui/Checkbox.vue'
import CodeEditor from '@/components/ui/CodeEditor.vue'
import DataTable from '@/components/ui/DataTable.vue'
import DescriptionList from '@/components/ui/DescriptionList.vue'
import DetailActions from '@/components/ui/DetailActions.vue'
import DetailPanel from '@/components/ui/DetailPanel.vue'
import Field from '@/components/ui/Field.vue'
import type { Json } from '@/components/ui/JsonEditor.vue'
import JsonEditor from '@/components/ui/JsonEditor.vue'
import MenuItem from '@/components/ui/MenuItem.vue'
import Page from '@/components/ui/Page.vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import PageSection from '@/components/ui/PageSection.vue'
import SkeletonRows from '@/components/ui/SkeletonRows.vue'
import { runAction } from '@/lib/action'
import { errorMessage } from '@/lib/errors'
import { formatDateTime, formatRelative } from '@/lib/format'
import type { Run, RunStatus, TestCase, TestOutcome, TestResult } from '@/queries/extensions'
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
const targetIds = ref<string[]>([])
const testsText = ref('')
const results = ref(new Map<string, TestResult>())
const running = ref<string | 'all' | null>(null)
const viewing = ref<Run | null>(null)
const viewingCase = ref<TestResult | null>(null)

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
    if (fresh || sameSet(targetIds.value, idsOf(synced))) targetIds.value = idsOf(loaded)
    if (fresh || testsText.value === casesText(synced?.tests))
      testsText.value = casesText(loaded.tests)
    synced = loaded
  },
  { immediate: true },
)
function pretty(value: unknown): string {
  return value === undefined ? '' : JSON.stringify(value, null, 2)
}

/** Cases as the editor shows them: name first, whatever order the API sent. */
function casesText(tests: TestCase[] | undefined): string {
  return tests === undefined
    ? ''
    : pretty(tests.map(({ name, input, expect }) => ({ name, input, expect })))
}

/** Cases as the server stores them: trimmed names, no `expect: null`. */
function normalized(tests: TestCase[]): TestCase[] {
  return tests.map(({ name, input, expect }) =>
    expect === null || expect === undefined
      ? { name: name.trim(), input }
      : { name: name.trim(), input, expect },
  )
}

/** JSON with object keys sorted, so key order never counts as a change. */
function canonical(value: unknown): string {
  return JSON.stringify(value, (_key, item: unknown) =>
    item && typeof item === 'object' && !Array.isArray(item)
      ? Object.fromEntries(Object.entries(item).sort(([x], [y]) => (x < y ? -1 : 1)))
      : item,
  )
}

/** The tests editor's cases, or why they can't be used. */
const cases = computed((): CasesResult => {
  try {
    return parseCases(JSON.parse(testsText.value) as Json)
  } catch {
    // JsonEditor shows the parse error itself.
    return { ok: false, error: '' }
  }
})

function addSampleCase() {
  const sample: TestCase = { name: 'Sample input', input: hook.value?.sample ?? {} }
  const current = cases.value.ok ? cases.value.cases : []
  testsText.value = casesText([...current, sample])
}

function idsOf(loaded: typeof script.value): string[] {
  return loaded?.targets.map((target) => target.id) ?? []
}

function sameSet(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((id) => b.includes(id))
}

const dirty = computed(
  () =>
    script.value !== undefined &&
    (name.value !== script.value.name ||
      source.value !== script.value.source ||
      enabled.value !== script.value.enabled ||
      !sameSet(targetIds.value, idsOf(script.value)) ||
      !cases.value.ok ||
      canonical(normalized(cases.value.cases)) !== canonical(normalized(script.value.tests))),
)

const CAPABILITY_NAMES: Record<string, string> = {
  log: 'ctx.log',
  secrets: 'ctx.secrets',
  fetch: 'ctx.fetch',
}

/** What the script runs for, in words. */
const runsFor = computed(() =>
  runsForText(script.value?.targets ?? [], hook.value?.target ?? undefined),
)

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

const sampleInput = computed(() => pretty(hook.value?.sample))

const OUTCOME_LABELS: Record<TestOutcome, string> = { pass: 'Pass', fail: 'Fail', error: 'Error' }
const OUTCOME_CLASSES: Record<TestOutcome, string> = {
  pass: 'badge-success',
  fail: 'badge-error',
  error: 'badge-warning',
}
const caseColumns = [
  { key: 'name', label: 'Case' },
  { key: 'outcome', label: 'Result' },
  { key: 'duration', label: 'Took' },
  { key: 'returned', label: 'Returned' },
  { key: 'actions', label: 'Actions', hidden: true, class: 'w-20 text-right' },
]

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
  const current = cases.value
  if (!current.ok) return
  return runAction({
    run: () =>
      update.mutateAsync({
        path: { workspace_id: props.workspaceId, script_id: props.scriptId },
        body: {
          name: name.value,
          source: source.value,
          enabled: enabled.value,
          tests: current.cases,
          ...(hook.value?.target && !sameSet(targetIds.value, idsOf(script.value))
            ? { target_ids: targetIds.value }
            : {}),
        },
      }),
    success: 'Script saved',
  })
}

/** Runs the given cases, or all of them, on the source in the editor. */
async function runCases(only?: TestCase) {
  if (!cases.value.ok) return
  running.value = only?.name ?? 'all'
  try {
    const report = await test.mutateAsync({
      path: { workspace_id: props.workspaceId, script_id: props.scriptId },
      body: { tests: only ? [only] : cases.value.cases, source: source.value },
    })
    const next = only ? new Map(results.value) : new Map<string, TestResult>()
    for (const result of report.items) next.set(result.name.trim(), result)
    results.value = next
  } finally {
    running.value = null
  }
  await refetchRuns()
}

function resultOf(testCase: TestCase): TestResult | undefined {
  return results.value.get(testCase.name.trim())
}

const passed = computed(
  () => [...results.value.values()].filter((r) => r.outcome === 'pass').length,
)

function returnedText(output: unknown): string {
  return output === null || output === undefined ? 'undefined' : JSON.stringify(output)
}

const viewingDiff = computed(() => {
  const result = viewingCase.value
  if (result?.outcome !== 'fail') return []
  return lineDiff(pretty(result.expect), pretty(result.run.output ?? null))
})

function deleteScript() {
  return runAction({
    confirm: {
      title: 'Delete this script?',
      message: 'Its run log goes too. You can’t undo this.',
      confirmLabel: 'Delete script',
    },
    run: async () => {
      await remove.mutateAsync({
        path: { workspace_id: props.workspaceId, script_id: props.scriptId },
      })
      return true
    },
    success: 'Script deleted',
  }).then((deleted) => {
    if (deleted) void router.push(`/w/${props.workspaceId}/p/${props.projectId}/extensions`)
  })
}

function summaryOf(run: Run): string {
  return run.error ?? run.logs.at(-1) ?? (run.output === null ? '' : JSON.stringify(run.output))
}
</script>

<template>
  <Page>
    <p v-if="scriptError" class="text-error">{{ errorMessage(scriptError) }}</p>
    <template v-else-if="script">
      <PageHeader
        :section="projectName"
        :title="script.name"
        :description="
          hook
            ? `Runs ${script.sync ? 'on' : 'after'} ${script.hook} for ${script.targets.length ? runsFor : runsFor.toLowerCase()}.`
            : 'This hook point isn’t compiled into this server, so the script never runs.'
        "
      >
        <DetailActions
          :save="{
            form: 'script-form',
            dirty,
            pending: update.asyncStatus.value === 'loading',
            blocked: cases.ok ? undefined : 'Fix the test cases to save',
          }"
        >
          <template #menu>
            <MenuItem danger @select="deleteScript">
              <Trash2 class="size-4" aria-hidden="true" />Delete script
            </MenuItem>
          </template>
        </DetailActions>
      </PageHeader>

      <DescriptionList :items="facts">
        <template #hook>
          <code class="text-xs">{{ script.hook }}</code>
        </template>
      </DescriptionList>

      <Card title="Script">
        <form id="script-form" class="flex flex-col gap-4" @submit.prevent="save">
          <div class="flex flex-wrap items-end gap-4">
            <Field label="Name" class="min-w-64 flex-1">
              <input v-model="name" name="script-name" required class="input w-full" />
            </Field>
            <div class="flex h-10 items-center">
              <Checkbox v-model="enabled" label="Enabled" />
            </div>
          </div>
          <TargetPicker
            v-if="hook?.target"
            v-model="targetIds"
            :workspace-id="workspaceId"
            :project-id="projectId"
            :kind="hook.target"
          />
          <div class="flex flex-col gap-1.5">
            <span class="text-sm font-medium">Source</span>
            <CodeEditor
              v-model="source"
              language="javascript"
              label="Script source"
              :min-lines="12"
              :max-lines="40"
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
        <details v-if="hook" class="rounded-field border border-base-300 px-3 py-2">
          <summary class="cursor-pointer text-sm font-medium">What the script can call</summary>
          <p class="mt-2 mb-3 text-sm text-base-content/80">
            Globals on every hook point, then the <code>ctx</code> members this point grants. Every
            helper takes and returns strings.
          </p>
          <ApiReference :entries="hook.api" />
        </details>
      </Card>

      <Card
        title="Tests"
        :description="`Saved with the script. Each case runs the source above, saved or not.${script.sync ? '' : ' Fetches are real.'}`"
      >
        <template #actions>
          <span
            v-if="results.size > 0"
            class="text-xs text-base-content/70 tabular-nums"
            role="status"
          >
            {{ passed }} of {{ results.size }} passed
          </span>
          <button
            type="button"
            class="btn gap-1.5 btn-sm"
            :disabled="running !== null || !cases.ok || cases.cases.length === 0"
            @click="runCases()"
          >
            <Play class="size-4" aria-hidden="true" />Run all
          </button>
        </template>
        <div class="flex flex-col gap-1.5">
          <span class="text-sm font-medium">Cases</span>
          <p class="text-xs text-base-content/70">
            A JSON array of <code>{"name", "input", "expect"}</code>. Without <code>expect</code>, a
            case passes when the run doesn’t fail.
          </p>
          <JsonEditor v-model="testsText" label="Test cases" :max-lines="30" />
          <p v-if="!cases.ok && cases.error" role="alert" class="text-xs text-error">
            {{ cases.error }}
          </p>
        </div>
        <p v-if="test.error.value" class="text-sm text-error">
          {{ errorMessage(test.error.value) }}
        </p>
        <DataTable
          :columns="caseColumns"
          :rows="cases.ok ? cases.cases : []"
          :row-key="(c) => c.name"
          label="Case results"
          :empty="{
            icon: FlaskConical,
            title: 'No test cases',
            message: 'Start from the hook point’s sample input.',
          }"
        >
          <template #cell-name="{ row }">
            <button
              v-if="resultOf(row)"
              type="button"
              class="link text-sm font-medium link-hover"
              @click="viewingCase = resultOf(row) ?? null"
            >
              {{ row.name }}
            </button>
            <span v-else class="text-sm font-medium">{{ row.name }}</span>
          </template>
          <template #cell-outcome="{ row }">
            <span
              v-if="resultOf(row)"
              class="badge badge-sm"
              :class="OUTCOME_CLASSES[resultOf(row)!.outcome]"
            >
              {{ OUTCOME_LABELS[resultOf(row)!.outcome] }}
            </span>
            <span v-else class="text-xs text-base-content/60">Not run</span>
          </template>
          <template #cell-duration="{ row }"
            ><span v-if="resultOf(row)" class="text-sm tabular-nums"
              >{{ resultOf(row)!.run.duration_ms }} ms</span
            ></template
          >
          <template #cell-returned="{ row }"
            ><code v-if="resultOf(row)" class="line-clamp-1 text-xs">{{
              resultOf(row)!.run.error ?? returnedText(resultOf(row)!.run.output)
            }}</code></template
          >
          <template #cell-actions="{ row }">
            <button
              type="button"
              class="btn btn-ghost btn-xs"
              :aria-label="`Run ${row.name}`"
              :disabled="running !== null"
              @click="runCases(row)"
            >
              <Play class="size-3.5" aria-hidden="true" />Run
            </button>
          </template>
          <template #empty-action>
            <button type="button" class="btn btn-sm" @click="addSampleCase">
              Add a sample case
            </button>
          </template>
        </DataTable>
      </Card>

      <PageSection title="Run log" description="The newest 100 runs, test runs included.">
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
              :title="formatDateTime(row.created_at)"
              @click="viewing = row"
            >
              {{ formatRelative(row.created_at) }}
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
      </PageSection>

      <DetailPanel
        :open="viewing !== null"
        :title="viewing ? `Run ${formatDateTime(viewing.created_at)}` : ''"
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

      <DetailPanel
        :open="viewingCase !== null"
        :title="viewingCase ? viewingCase.name : ''"
        @close="viewingCase = null"
      >
        <template v-if="viewingCase">
          <p class="mb-3 text-sm">
            <span class="badge badge-sm" :class="OUTCOME_CLASSES[viewingCase.outcome]">
              {{ OUTCOME_LABELS[viewingCase.outcome] }}
            </span>
            {{ STATUS_LABELS[viewingCase.run.status] }} · {{ viewingCase.run.duration_ms }} ms
          </p>
          <p v-if="viewingCase.run.error" class="mb-3 text-sm text-error">
            {{ viewingCase.run.error }}
          </p>
          <template v-if="viewingDiff.length">
            <h3 class="mb-1 text-xs font-semibold">Expected (-) and returned (+)</h3>
            <pre
              class="mb-3 overflow-x-auto rounded-field bg-base-200 p-2 text-xs"
              data-testid="case-diff"
            ><span
              v-for="(line, i) in viewingDiff"
              :key="i"
              class="block"
              :class="{ 'text-error': line.kind === 'removed', 'text-success': line.kind === 'added' }"
            >{{ line.kind === 'removed' ? '-' : line.kind === 'added' ? '+' : ' ' }} {{ line.text }}</span></pre>
          </template>
          <h3 class="mb-1 text-xs font-semibold">Logs</h3>
          <pre class="mb-3 overflow-x-auto rounded-field bg-base-200 p-2 text-xs">{{
            viewingCase.run.logs.length ? viewingCase.run.logs.join('\n') : '(none)'
          }}</pre>
          <h3 class="mb-1 text-xs font-semibold">Returned</h3>
          <pre class="overflow-x-auto rounded-field bg-base-200 p-2 text-xs">{{
            viewingCase.run.output === null ? 'undefined' : pretty(viewingCase.run.output)
          }}</pre>
        </template>
      </DetailPanel>
    </template>
    <SkeletonRows v-else :rows="6" />
  </Page>
</template>
