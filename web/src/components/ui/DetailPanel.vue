<script setup lang="ts">
import Overlay from './Overlay.vue'

/** A read-only side panel, such as a record's details. */
defineProps<{ open: boolean; title: string; description?: string }>()
const emit = defineEmits<{ close: [] }>()
</script>

<template>
  <Overlay
    :open="open"
    variant="panel"
    :title="title"
    :description="description"
    @close="emit('close')"
  >
    <slot />
    <template v-if="$slots.actions" #footer>
      <slot name="actions" />
    </template>
  </Overlay>
</template>
