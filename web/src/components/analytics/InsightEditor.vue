<script setup lang="ts">
import { refDebounced } from '@vueuse/core'
import { Play } from 'lucide-vue-next'
import { computed, ref, shallowRef } from 'vue'

import JsonEditor from '@/components/ui/JsonEditor.vue'
import SegmentedControl from '@/components/ui/SegmentedControl.vue'
import type { DateRange } from '@/lib/range'
import { type InsightQuery, useInsightResult } from '@/queries/analytics'

import {
  builderOfCode,
  codeOf,
  type EditorMode,
  type InsightEditorState,
  runOfCode,
} from './insightCode'
import InsightControls from './InsightControls.vue'
import { isRunnable, queryOf } from './insightDraft'
import InsightResultView from './InsightResultView.vue'
import RangeToolbar from './RangeToolbar.vue'

/**
 * The insight editor shared by the new and saved insight pages: the range
 * and the Builder or Code switch, the builder or the query JSON, then the
 * result. The builder runs once typing pauses; Code runs on Run or
 * Cmd/Ctrl+Enter, since half-typed JSON would only fail.
 */
const props = defineProps<{ workspaceId: string; projectId: string; today: string }>()
const range = defineModel<DateRange>('range', { required: true })
const editor = defineModel<InsightEditorState>('editor', { required: true })

const MODES: { value: EditorMode; label: string }[] = [
  { value: 'builder', label: 'Builder' },
  { value: 'code', label: 'Code' },
]
const notice = ref('')

const draft = computed({
  get: () => editor.value.draft,
  set: (value) => (editor.value = { ...editor.value, draft: value }),
})
const code = computed({
  get: () => editor.value.code,
  set: (value) => {
    notice.value = ''
    editor.value = { ...editor.value, code: value }
  },
})

const TYPING_PAUSE_MS = 300
// The query and whether it can run come from the same settled draft, so
// a half-made draft never reaches the server.
const settledDraft = refDebounced(draft, TYPING_PAUSE_MS)
const ran = shallowRef<{ query: InsightQuery; code: string }>()
const shown = computed(() =>
  editor.value.mode === 'code' ? ran.value?.query : queryOf(settledDraft.value),
)
const runnable = computed(() =>
  editor.value.mode === 'code' ? ran.value !== undefined : isRunnable(settledDraft.value),
)
const result = useInsightResult(
  () => props.workspaceId,
  () => props.projectId,
  range,
  () => shown.value ?? { kind: 'people' },
  runnable,
)

function toCode() {
  const text = codeOf(editor.value.draft)
  editor.value = { ...editor.value, mode: 'code', code: text }
  ran.value = isRunnable(editor.value.draft)
    ? { query: queryOf(editor.value.draft), code: text }
    : undefined
  notice.value = ''
}

function toBuilder() {
  const next = builderOfCode(editor.value)
  editor.value = next.editor
  notice.value = next.notice
}

const mode = computed({
  get: () => editor.value.mode,
  set: (value: EditorMode) => {
    if (value === editor.value.mode) return
    if (value === 'code') toCode()
    else toBuilder()
  },
})

function run() {
  const { query, notice: error } = runOfCode(editor.value.code)
  notice.value = error
  if (!query) return
  const same = ran.value && JSON.stringify(ran.value.query) === JSON.stringify(query)
  ran.value = { query, code: editor.value.code }
  if (same) void result.refetch()
}

// Capture runs before CodeMirror's own Mod-Enter binding (insert a line).
function onCodeKey(event: KeyboardEvent) {
  if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
    event.preventDefault()
    event.stopPropagation()
    run()
  }
}

const stale = computed(() => editor.value.mode === 'code' && ran.value?.code !== editor.value.code)
</script>

<template>
  <RangeToolbar v-model:range="range" :today="today">
    <SegmentedControl v-model="mode" :options="MODES" label="Query editor" class="ml-auto" />
  </RangeToolbar>
  <p v-if="notice" role="alert" class="text-sm text-error">{{ notice }}</p>
  <InsightControls
    v-if="editor.mode === 'builder'"
    v-model="draft"
    :workspace-id="workspaceId"
    :project-id="projectId"
    :range="range"
  />
  <section
    v-else
    class="flex flex-col gap-3 rounded-box border border-base-300 bg-base-100 p-4"
    aria-label="Query JSON"
    @keydown.capture="onCodeKey"
  >
    <JsonEditor v-model="code" label="Insight query JSON" :min-lines="8" />
    <div class="flex flex-wrap items-center gap-3">
      <button type="button" class="btn gap-1.5 btn-sm" @click="run">
        <Play class="size-4" aria-hidden="true" />Run
      </button>
      <p class="text-sm text-base-content/70">
        <kbd class="kbd kbd-sm">Cmd</kbd> or <kbd class="kbd kbd-sm">Ctrl</kbd> +
        <kbd class="kbd kbd-sm">Enter</kbd> runs it.
        <template v-if="stale && ran">The result is from the last run.</template>
      </p>
    </div>
  </section>
  <section
    class="flex flex-col gap-4 rounded-box border border-base-300 bg-base-100 p-4"
    aria-label="Result"
  >
    <p
      v-if="editor.mode === 'builder' && !isRunnable(draft)"
      class="py-6 text-center text-sm text-base-content/70"
    >
      Pick an event to see its numbers.
    </p>
    <p
      v-else-if="editor.mode === 'code' && !runnable"
      class="py-6 text-center text-sm text-base-content/70"
    >
      Run the query to see its numbers.
    </p>
    <!-- In the builder, a draft that hasn't settled yet is a result on its way. -->
    <InsightResultView
      v-else
      :result="result.data.value"
      :loading="result.isLoading.value || !runnable"
      :error="result.error.value"
    />
  </section>
</template>
