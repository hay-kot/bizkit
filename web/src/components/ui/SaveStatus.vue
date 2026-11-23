<script setup lang="ts">
import { CircleCheck, CircleDot } from 'lucide-vue-next'

/** "Saved" or "Unsaved changes" next to a Save button. */
defineProps<{ dirty: boolean }>()
</script>

<template>
  <span
    class="flex items-center gap-1.5 text-xs"
    :class="dirty ? 'text-warning' : 'text-base-content/60'"
    role="status"
  >
    <component :is="dirty ? CircleDot : CircleCheck" class="size-3.5" aria-hidden="true" />
    {{ dirty ? 'Unsaved changes' : 'Saved' }}
  </span>
</template>
