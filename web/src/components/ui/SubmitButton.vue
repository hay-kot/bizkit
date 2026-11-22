<script setup lang="ts">
/** A primary submit button that shows a spinner and disables itself while `pending`. */
withDefaults(
  defineProps<{ pending?: boolean; disabled?: boolean; form?: string; size?: 'sm' | 'md' }>(),
  {
    pending: false,
    disabled: false,
    form: undefined,
    size: 'sm',
  },
)
</script>

<template>
  <button
    type="submit"
    :form="form"
    class="btn btn-primary"
    :class="size === 'sm' ? 'btn-sm' : ''"
    :disabled="disabled || pending"
    :aria-busy="pending"
  >
    <span v-if="pending" class="loading loading-xs loading-spinner" aria-hidden="true"></span>
    <slot />
  </button>
</template>
