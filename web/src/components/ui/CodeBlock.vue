<script lang="ts">
type Highlighter = typeof import('./codeHighlight')

// Shared by every block, so once one block has loaded the highlighter and a
// parser, the next block with that language colors on its first render.
let highlighter: Highlighter | undefined

function withoutClass(attrs: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(Object.entries(attrs).filter(([key]) => key !== 'class'))
}
</script>

<script setup lang="ts">
import { ref, watch } from 'vue'

import type { CodeLanguage, CodeRun } from './codeHighlight'
import CopyButton from './CopyButton.vue'

/**
 * Static code with syntax colors. The text shows at once; the colors follow
 * when the language's parser chunk loads, so pages without code never load
 * it. `copy` adds a copy button with that label; `wrap` breaks long lines
 * instead of scrolling. A `class` lands on the outer row (with the copy
 * button); other attributes land on the `<pre>`.
 */
const props = withDefaults(
  defineProps<{
    code: string
    language: CodeLanguage
    copy?: string
    wrap?: boolean
    size?: 'xs' | 'sm'
  }>(),
  { copy: undefined, wrap: false, size: 'xs' },
)
defineOptions({ inheritAttrs: false })

const runs = ref<CodeRun[] | null>(null)
let latest = 0

watch(
  () => [props.code, props.language] as const,
  async ([code, language]) => {
    const request = ++latest
    runs.value = highlighter?.highlightLoaded(code, language) ?? null
    if (runs.value) return
    try {
      highlighter ??= await import('./codeHighlight')
      const result = await highlighter.highlight(code, language)
      if (request === latest) runs.value = result
    } catch {
      // The plain text stays; it reads fine without colors.
    }
  },
  { immediate: true },
)
</script>

<template>
  <div class="flex min-w-0 items-start gap-2" :class="$attrs.class">
    <pre
      v-bind="withoutClass($attrs)"
      tabindex="0"
      class="min-w-0 flex-1 overflow-x-auto rounded-field bg-base-200 p-3 font-mono"
      :class="[
        size === 'sm' ? 'text-sm' : 'text-xs',
        { 'wrap-break-word whitespace-pre-wrap': wrap },
      ]"
    ><code v-if="runs"><span v-for="(run, i) in runs" :key="i" :class="run.class || undefined">{{ run.text }}</span></code><code v-else>{{ code }}</code></pre>
    <CopyButton v-if="copy" :text="code" :label="copy" />
  </div>
</template>
