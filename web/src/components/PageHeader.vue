<script setup lang="ts">
defineProps<{ title: string; description?: string; section?: string }>()
</script>

<template>
  <div class="mb-6 flex flex-wrap items-end justify-between gap-4">
    <div class="min-w-0">
      <p v-if="section" class="text-xs font-medium tracking-wide text-base-content/70 uppercase">
        {{ section }}
      </p>
      <h1 class="text-2xl font-semibold tracking-tight">{{ title }}</h1>
      <p v-if="description" class="mt-1 text-sm text-base-content/80">{{ description }}</p>
    </div>
    <div class="flex gap-2"><slot /></div>
  </div>
</template>
