<script setup lang="ts">
import { defineAsyncComponent } from 'vue'

import SkeletonRows from './SkeletonRows.vue'

/**
 * A code editor for JavaScript or JSON. CodeMirror loads in its own chunk on
 * first use, so pages without an editor don't download it.
 */
withDefaults(
  defineProps<{
    label: string
    language: 'javascript' | 'json'
    readonly?: boolean
    minLines?: number
    maxLines?: number
  }>(),
  { readonly: false, minLines: 4, maxLines: 30 },
)
const model = defineModel<string>({ required: true })

const CodeMirrorView = defineAsyncComponent({
  loader: () => import('./CodeMirrorView.vue'),
  loadingComponent: SkeletonRows,
  delay: 0,
})
</script>

<template>
  <CodeMirrorView
    v-model="model"
    :label="label"
    :language="language"
    :readonly="readonly"
    :min-lines="minLines"
    :max-lines="maxLines"
  />
</template>
