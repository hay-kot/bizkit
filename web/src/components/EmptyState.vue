<script setup lang="ts">
defineProps<{ message: string }>()
</script>

<template>
  <div class="rounded-box border border-dashed border-base-300 p-8 text-center text-sm">
    {{ message }}
  </div>
</template>
