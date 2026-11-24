<script setup lang="ts">
/**
 * A labelled form field: label, control (default slot), hint and error.
 * Pages are full width, so a field caps its own width to stay readable:
 * `lg` (default) for text inputs, selects and textareas, `md` for short
 * values such as names and IDs, `full` for a control that needs the room.
 */
const props = withDefaults(
  defineProps<{ label: string; hint?: string; error?: string; width?: 'md' | 'lg' | 'full' }>(),
  { hint: undefined, error: undefined, width: 'lg' },
)
const widths = { md: 'max-w-md', lg: 'max-w-3xl', full: '' }
</script>

<template>
  <label class="flex flex-col gap-1.5" :class="widths[props.width]">
    <span class="text-sm font-medium">{{ label }}</span>
    <slot />
    <span v-if="error" class="text-xs text-error">{{ error }}</span>
    <span v-else-if="hint" class="text-xs text-base-content/70">{{ hint }}</span>
  </label>
</template>
