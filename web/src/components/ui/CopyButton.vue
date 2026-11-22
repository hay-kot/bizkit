<script setup lang="ts">
import { useClipboard } from '@vueuse/core'
import { Check, Copy } from 'lucide-vue-next'

const props = defineProps<{ text: string; label?: string }>()
const { copy, copied } = useClipboard({ copiedDuring: 1500, legacy: true })
</script>

<template>
  <button
    type="button"
    class="btn gap-1.5 btn-ghost btn-sm"
    :aria-label="props.label ?? 'Copy'"
    @click="copy(text)"
  >
    <component :is="copied ? Check : Copy" class="size-4" aria-hidden="true" />
    <span>{{ copied ? 'Copied' : 'Copy' }}</span>
  </button>
</template>
