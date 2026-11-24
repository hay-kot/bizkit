<script setup lang="ts">
import { computed } from 'vue'

import type { RequestStatus } from '@/queries/signing'

const props = defineProps<{ status: RequestStatus }>()

const look = computed(() => {
  switch (props.status) {
    case 'completed':
      return 'badge-success'
    case 'sent':
      return 'badge-info'
    case 'failed':
      return 'badge-error'
    default:
      return 'badge-ghost'
  }
})
</script>

<template>
  <span class="badge badge-sm" :class="look">{{ status }}</span>
</template>
