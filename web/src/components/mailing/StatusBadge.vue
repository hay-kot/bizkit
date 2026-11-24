<script setup lang="ts">
import { computed } from 'vue'

import type {
  BroadcastStatus,
  ContactStatus,
  DeliveryStatus,
  EnrollmentStatus,
} from '@/queries/mailing'

const props = defineProps<{
  status: ContactStatus | BroadcastStatus | DeliveryStatus | EnrollmentStatus
}>()

const look = computed(() => {
  switch (props.status) {
    case 'subscribed':
    case 'sent':
    case 'completed':
      return 'badge-success'
    case 'active':
      return 'badge-info'
    case 'pending':
    case 'queued':
    case 'sending':
      return 'badge-warning'
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
