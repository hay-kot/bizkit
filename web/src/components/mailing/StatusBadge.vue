<script setup lang="ts">
import { computed } from 'vue'

import type {
  BroadcastStatus,
  ContactStatus,
  DeliveryEvent,
  DeliveryStatus,
  EnrollmentStatus,
  SuppressionReason,
} from '@/queries/mailing'

const props = defineProps<{
  status:
    | ContactStatus
    | BroadcastStatus
    | DeliveryStatus
    | DeliveryEvent
    | EnrollmentStatus
    | SuppressionReason
}>()

const look = computed(() => {
  switch (props.status) {
    case 'subscribed':
    case 'sent':
    case 'completed':
      return 'badge-success'
    case 'active':
    case 'scheduled':
      return 'badge-info'
    case 'pending':
    case 'queued':
    case 'sending':
      return 'badge-warning'
    case 'failed':
    case 'paused':
    case 'bounced':
    case 'complained':
    case 'bounce':
    case 'complaint':
      return 'badge-error'
    default:
      return 'badge-ghost'
  }
})
</script>

<template>
  <span class="badge badge-sm" :class="look">{{ status }}</span>
</template>
