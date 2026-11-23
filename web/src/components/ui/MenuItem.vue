<script setup lang="ts">
import { computed } from 'vue'
import type { RouteLocationRaw } from 'vue-router'

/** An item in a `Menu`. `danger` marks a destructive action. */
const props = defineProps<{ to?: RouteLocationRaw; danger?: boolean }>()
defineEmits<{ select: [] }>()
const itemClass = computed(() => [
  'flex w-full items-center gap-2 rounded-field px-2 py-1.5 text-left text-sm transition-colors hover:bg-base-200 focus-visible:bg-base-200 focus-visible:outline-none',
  props.danger && 'text-error',
])
</script>

<template>
  <RouterLink v-if="to" :to="to" role="menuitem" :class="itemClass"><slot /></RouterLink>
  <button v-else type="button" role="menuitem" :class="itemClass" @click="$emit('select')">
    <slot />
  </button>
</template>
