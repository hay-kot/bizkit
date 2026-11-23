<script setup lang="ts">
import { ref, useId } from 'vue'

/**
 * A monospace textarea for script source. Tab inserts two spaces; Esc then
 * Tab moves focus on, so keyboard users can still leave the field.
 */
defineProps<{ name: string; rows?: number; label: string }>()
const model = defineModel<string>({ required: true })
const released = ref(false)
const hintId = useId()

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    released.value = true
    return
  }
  const wasReleased = released.value
  released.value = false
  if (event.key !== 'Tab' || event.shiftKey || event.altKey || event.metaKey || event.ctrlKey) {
    return
  }
  if (wasReleased) return
  event.preventDefault()
  const area = event.target as HTMLTextAreaElement
  const { selectionStart: start, selectionEnd: end } = area
  model.value = `${model.value.slice(0, start)}  ${model.value.slice(end)}`
  requestAnimationFrame(() => {
    area.selectionStart = start + 2
    area.selectionEnd = start + 2
  })
}
</script>

<template>
  <textarea
    v-model="model"
    :name="name"
    :rows="rows ?? 18"
    :aria-label="label"
    :aria-describedby="hintId"
    spellcheck="false"
    autocapitalize="off"
    class="textarea w-full font-mono text-xs leading-relaxed"
    @keydown="onKeydown"
  ></textarea>
  <p :id="hintId" class="mt-1 text-xs text-base-content/70">
    Tab indents. Press Esc, then Tab, to move on.
  </p>
</template>
