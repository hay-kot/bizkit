<script setup lang="ts">
/** A file picker. The model is the chosen file, or null. */
defineProps<{ accept?: string; name?: string; required?: boolean }>()
const model = defineModel<File | null>({ required: true })

function onChange(event: Event) {
  const input = event.target as HTMLInputElement
  model.value = input.files?.[0] ?? null
}
</script>

<template>
  <input
    type="file"
    class="file-input w-full"
    :accept="accept"
    :name="name"
    :required="required"
    @change="onChange"
  />
</template>
