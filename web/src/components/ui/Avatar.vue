<script setup lang="ts">
import { computed } from 'vue'

/** A square with the first letter of `name`, tinted from a fixed set by the name. */
const props = withDefaults(defineProps<{ name: string; size?: 'sm' | 'md' | 'lg' }>(), {
  size: 'md',
})
const tints = [
  'bg-primary text-primary-content',
  'bg-secondary text-secondary-content',
  'bg-accent text-accent-content',
  'bg-info text-info-content',
  'bg-success text-success-content',
  'bg-neutral text-neutral-content',
]
const tint = computed(() => {
  let hash = 0
  for (const char of props.name) hash = (hash * 31 + char.charCodeAt(0)) >>> 0
  return tints[hash % tints.length]
})
const sizes = { sm: 'size-6 text-xs', md: 'size-8 text-sm', lg: 'size-10 text-base' }
</script>

<template>
  <span
    class="inline-flex shrink-0 items-center justify-center rounded-field font-semibold uppercase"
    :class="[tint, sizes[size]]"
    aria-hidden="true"
  >
    {{ name.trim().charAt(0) || '?' }}
  </span>
</template>
