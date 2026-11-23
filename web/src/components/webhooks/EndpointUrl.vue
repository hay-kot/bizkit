<script setup lang="ts">
import CopyButton from '@/components/ui/CopyButton.vue'

/** An endpoint's capture URL, cut to fit a table cell, with a copy button. */
defineProps<{ url: string; name: string }>()
</script>

<template>
  <div class="flex items-center gap-1">
    <code class="max-w-xs truncate text-xs">{{ url }}</code>
    <CopyButton :text="url" :label="`Copy the ${name} URL`" />
  </div>
</template>
