<script setup lang="ts">
import type { Delivery } from '@/queries/webhooks'

/** The answer a forward or replay got: its status, or "Failed" without one. */
defineProps<{ delivery: Delivery }>()
</script>

<template>
  <span
    class="badge badge-sm"
    :class="delivery.error ? 'badge-error' : 'badge-success'"
    :title="delivery.error ?? undefined"
  >
    {{ delivery.status ?? 'Failed' }}
  </span>
</template>
