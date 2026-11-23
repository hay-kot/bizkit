<script setup lang="ts">
import { Ellipsis } from 'lucide-vue-next'

import Menu from './Menu.vue'

/**
 * The "..." button in a page header for secondary and destructive actions.
 * Put `MenuItem`s in the default slot, with destructive ones (`danger`) last.
 */
withDefaults(defineProps<{ label?: string }>(), { label: 'More actions' })
</script>

<template>
  <Menu :label="label" align="end">
    <template #trigger="{ open, toggle, controls }">
      <button
        type="button"
        class="btn btn-square btn-ghost btn-sm"
        :aria-label="label"
        aria-haspopup="menu"
        :aria-expanded="open"
        :aria-controls="controls"
        @click="toggle"
      >
        <Ellipsis class="size-4" aria-hidden="true" />
      </button>
    </template>
    <slot />
  </Menu>
</template>
