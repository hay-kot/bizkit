<script setup lang="ts">
import { Check } from 'lucide-vue-next'

/**
 * A real checkbox input, restyled: no bounce, just a quick color change.
 * Bind an array with `value` for groups, or a boolean on its own.
 */
defineProps<{ value?: string; label?: string; disabled?: boolean }>()
const model = defineModel<boolean | string[]>({ required: true })
</script>

<template>
  <label
    class="group inline-flex cursor-pointer items-center gap-2 text-sm select-none has-disabled:cursor-not-allowed has-disabled:opacity-60"
  >
    <span class="relative inline-flex size-4 shrink-0">
      <input
        v-model="model"
        type="checkbox"
        :value="value"
        :disabled="disabled"
        class="peer size-4 cursor-pointer appearance-none rounded-sm border border-base-content/40 bg-base-100 transition-colors duration-100 group-hover:border-base-content/70 checked:border-primary checked:bg-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-secondary"
      />
      <Check
        class="pointer-events-none absolute inset-0 size-4 p-0.5 text-primary-content opacity-0 transition-opacity duration-100 peer-checked:opacity-100"
        :stroke-width="3"
        aria-hidden="true"
      />
    </span>
    <slot>{{ label }}</slot>
  </label>
</template>
