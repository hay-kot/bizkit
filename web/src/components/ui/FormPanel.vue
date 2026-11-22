<script setup lang="ts">
import { useId } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'

import Overlay from './Overlay.vue'
import SubmitButton from './SubmitButton.vue'

/**
 * A form in a side panel (or a dialog): fields in the default slot, the API
 * error under them, and Cancel plus a submit button that shows `pending`.
 */
withDefaults(
  defineProps<{
    open: boolean
    title: string
    description?: string
    submitLabel: string
    pending?: boolean
    error?: unknown
    disabled?: boolean
    variant?: 'panel' | 'dialog'
  }>(),
  { description: undefined, pending: false, error: undefined, disabled: false, variant: 'panel' },
)
const emit = defineEmits<{ close: []; submit: [] }>()
const formId = useId()
</script>

<template>
  <Overlay
    :open="open"
    :variant="variant"
    :title="title"
    :description="description"
    @close="emit('close')"
  >
    <form :id="formId" class="flex flex-col gap-4" @submit.prevent="emit('submit')">
      <slot />
      <ErrorAlert :error="error" />
    </form>
    <template #footer>
      <button type="button" class="btn btn-ghost btn-sm" @click="emit('close')">Cancel</button>
      <SubmitButton :form="formId" :pending="pending" :disabled="disabled">{{
        submitLabel
      }}</SubmitButton>
    </template>
  </Overlay>
</template>
