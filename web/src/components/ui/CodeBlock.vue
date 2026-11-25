<script setup lang="ts">
import { ref, watch } from 'vue'

import type { CodeLanguage, CodeRun } from './codeHighlight'
import CopyButton from './CopyButton.vue'

/**
 * Static code with syntax colors. The text shows at once; the colors follow
 * when the language's parser chunk loads, so pages without code never load
 * it. `copy` adds a copy button with that label; `wrap` breaks long lines
 * instead of scrolling.
 */
const props = withDefaults(
  defineProps<{ code: string; language: CodeLanguage; copy?: string; wrap?: boolean }>(),
  { copy: undefined, wrap: false },
)
defineOptions({ inheritAttrs: false })

const runs = ref<CodeRun[] | null>(null)
let latest = 0

watch(
  () => [props.code, props.language] as const,
  async ([code, language]) => {
    const request = ++latest
    runs.value = null
    const { highlight } = await import('./codeHighlight')
    const result = await highlight(code, language)
    if (request === latest) runs.value = result
  },
  { immediate: true },
)
</script>

<template>
  <div class="flex min-w-0 items-start gap-2">
    <pre
      v-bind="$attrs"
      tabindex="0"
      class="min-w-0 flex-1 overflow-x-auto rounded-field bg-base-200 p-3 font-mono text-xs"
      :class="{ 'wrap-break-word whitespace-pre-wrap': wrap }"
    ><code v-if="runs"><span v-for="(run, i) in runs" :key="i" :class="run.class || undefined">{{ run.text }}</span></code><code v-else>{{ code }}</code></pre>
    <CopyButton v-if="copy" :text="code" :label="copy" />
  </div>
</template>
