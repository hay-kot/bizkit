<script setup lang="ts">
import type { RouteLocationRaw } from 'vue-router'

defineProps<{ to?: RouteLocationRaw }>()
defineEmits<{ select: [] }>()
const itemClass =
  'flex w-full items-center gap-2 rounded-field px-2 py-1.5 text-left text-sm transition-colors hover:bg-base-200 focus-visible:bg-base-200 focus-visible:outline-none'
</script>

<template>
  <RouterLink v-if="to" :to="to" role="menuitem" :class="itemClass"><slot /></RouterLink>
  <button v-else type="button" role="menuitem" :class="itemClass" @click="$emit('select')">
    <slot />
  </button>
</template>
