<script setup lang="ts">
withDefaults(defineProps<{ rows?: number }>(), { rows: 3 })
</script>

<template>
  <div class="flex flex-col gap-3 py-2" aria-busy="true" aria-label="Loading">
    <div v-for="row in rows" :key="row" class="flex items-center gap-4">
      <div class="h-4 w-1/3 animate-pulse rounded-field bg-base-300"></div>
      <div class="h-4 w-1/4 animate-pulse rounded-field bg-base-300"></div>
      <div class="ml-auto h-4 w-16 animate-pulse rounded-field bg-base-300"></div>
    </div>
  </div>
</template>
