<script setup lang="ts">
/**
 * A labelled form field: label, control (default slot), hint and error.
 * `width="md"` caps the field at a readable width, for fields in a
 * full-width settings card.
 */
withDefaults(
  defineProps<{ label: string; hint?: string; error?: string; width?: 'full' | 'md' }>(),
  {
    hint: undefined,
    error: undefined,
    width: 'full',
  },
)
</script>

<template>
  <label class="flex flex-col gap-1.5" :class="width === 'md' ? 'max-w-md' : ''">
    <span class="text-sm font-medium">{{ label }}</span>
    <slot />
    <span v-if="error" class="text-xs text-error">{{ error }}</span>
    <span v-else-if="hint" class="text-xs text-base-content/70">{{ hint }}</span>
  </label>
</template>
