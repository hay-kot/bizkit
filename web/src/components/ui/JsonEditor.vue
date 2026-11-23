<script setup lang="ts">
import { computed, useId } from 'vue'

import CodeEditor from './CodeEditor.vue'

export type Json = string | number | boolean | Json[] | { [key: string]: Json } | null
export type ParseResult = { ok: true; value: Json } | { ok: false; error: string }

withDefaults(defineProps<{ label: string; minLines?: number; maxLines?: number }>(), {
  minLines: 4,
  maxLines: 30,
})
const model = defineModel<string>({ required: true })
const errorId = useId()

const parsed = computed<ParseResult>(() => {
  try {
    return { ok: true, value: JSON.parse(model.value) as Json }
  } catch (err) {
    return { ok: false, error: err instanceof Error ? err.message : String(err) }
  }
})

function format() {
  if (parsed.value.ok) model.value = JSON.stringify(parsed.value.value, null, 2)
}

defineExpose({ parsed })
</script>

<template>
  <div>
    <CodeEditor
      v-model="model"
      language="json"
      :label="label"
      :min-lines="minLines"
      :max-lines="maxLines"
    />
    <div class="mt-1 flex items-start gap-2">
      <p v-if="!parsed.ok" :id="errorId" role="alert" class="flex-1 text-xs text-error">
        Invalid JSON: {{ parsed.error }}
      </p>
      <button
        type="button"
        class="btn ml-auto btn-ghost btn-xs"
        :disabled="!parsed.ok"
        @click="format"
      >
        Format
      </button>
    </div>
  </div>
</template>
