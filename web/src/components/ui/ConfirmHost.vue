<script setup lang="ts">
import { pendingConfirm } from '@/lib/confirm'

import Overlay from './Overlay.vue'
</script>

<template>
  <Overlay
    :open="!!pendingConfirm"
    :title="pendingConfirm?.title ?? ''"
    @close="pendingConfirm?.resolve(false)"
  >
    <p class="text-sm">{{ pendingConfirm?.message }}</p>
    <template #footer>
      <button type="button" class="btn btn-ghost btn-sm" @click="pendingConfirm?.resolve(false)">
        Cancel
      </button>
      <button
        type="button"
        class="btn btn-sm"
        :class="pendingConfirm?.danger ? 'btn-error' : 'btn-primary'"
        data-autofocus
        @click="pendingConfirm?.resolve(true)"
      >
        {{ pendingConfirm?.confirmLabel }}
      </button>
    </template>
  </Overlay>
</template>
