<script setup lang="ts">
import { TriangleAlert } from 'lucide-vue-next'

import CopyButton from '@/components/ui/CopyButton.vue'
import Overlay from '@/components/ui/Overlay.vue'

/** Shows a new API key's token, the one time it is visible. */
defineProps<{ token: string | undefined }>()
const emit = defineEmits<{ close: [] }>()
</script>

<template>
  <Overlay :open="!!token" variant="panel" title="Key created" @close="emit('close')">
    <div v-if="token" class="flex flex-col gap-4" data-testid="new-token">
      <div role="alert" class="alert alert-warning">
        <TriangleAlert class="size-4" aria-hidden="true" />
        <span>Copy this token now. You won't see it again.</span>
      </div>
      <div class="flex items-center gap-2 rounded-field border border-base-300 bg-base-200 p-2">
        <code class="flex-1 text-sm break-all select-all" data-testid="token-value">{{
          token
        }}</code>
        <CopyButton :text="token" label="Copy token" />
      </div>
      <p class="text-sm">Send it as <code>Authorization: Bearer &lt;token&gt;</code>.</p>
    </div>
    <template #footer>
      <button type="button" class="btn btn-primary btn-sm" data-autofocus @click="emit('close')">
        Done
      </button>
    </template>
  </Overlay>
</template>
