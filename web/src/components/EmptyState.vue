<script setup lang="ts">
import type { Component } from 'vue'

defineProps<{ title: string; message?: string; icon?: Component }>()
</script>

<template>
  <div
    class="flex flex-col items-center gap-2 rounded-box border border-dashed border-base-300 px-6 py-12 text-center"
  >
    <span
      v-if="icon"
      class="mb-1 inline-flex size-10 items-center justify-center rounded-full bg-base-200"
    >
      <component :is="icon" class="size-5" aria-hidden="true" />
    </span>
    <p class="font-medium">{{ title }}</p>
    <p v-if="message" class="max-w-sm text-sm text-base-content/80">{{ message }}</p>
    <div v-if="$slots.default" class="mt-3"><slot /></div>
  </div>
</template>
