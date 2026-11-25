<script setup lang="ts">
import { ref, watch } from 'vue'

/** A file picker. The model is the chosen file, or null; setting it to null clears the picker. */
defineProps<{ accept?: string; name?: string; required?: boolean }>()
const model = defineModel<File | null>({ required: true })
const input = ref<HTMLInputElement | null>(null)

watch(model, (file) => {
  if (file === null && input.value) input.value.value = ''
})

function onChange(event: Event) {
  const target = event.target as HTMLInputElement
  model.value = target.files?.[0] ?? null
}
</script>

<template>
  <input
    ref="input"
    type="file"
    class="file-input w-full"
    :accept="accept"
    :name="name"
    :required="required"
    @change="onChange"
  />
</template>
