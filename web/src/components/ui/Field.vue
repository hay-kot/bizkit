<script setup lang="ts">
/** A labelled form field: label, control (default slot), hint and error. */
defineProps<{ label: string; hint?: string; error?: string }>()
</script>

<template>
  <label class="flex flex-col gap-1.5">
    <span class="text-sm font-medium">{{ label }}</span>
    <slot />
    <span v-if="error" class="text-xs text-error">{{ error }}</span>
    <span v-else-if="hint" class="text-xs text-base-content/70">{{ hint }}</span>
  </label>
</template>
