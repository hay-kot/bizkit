<script setup lang="ts">
withDefaults(defineProps<{ label?: string }>(), { label: 'Loading' })
</script>

<template>
  <span class="loading loading-spinner" role="status" :aria-label="label"></span>
</template>
