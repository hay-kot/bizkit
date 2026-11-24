<script setup lang="ts">
/**
 * The root of every page. It sets the width and the space between the
 * page's top-level blocks, so pages add no margins between them.
 *
 * - `wide` (default): the layout's full content width. Every page in the
 *   workspace shell uses it, so blocks line up from page to page.
 * - `compact`: a page that is one small card without a header, such as
 *   Invite, centered.
 */
withDefaults(defineProps<{ width?: 'wide' | 'compact' }>(), { width: 'wide' })
const widths = {
  wide: 'w-full',
  compact: 'mx-auto mt-8 w-full max-w-md',
}
</script>

<template>
  <div class="flex flex-col gap-8" :class="widths[width]"><slot /></div>
</template>
