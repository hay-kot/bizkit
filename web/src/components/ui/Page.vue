<script setup lang="ts">
/**
 * The root of every page. It sets the width and the space between the
 * page's top-level blocks, so pages add no margins between them. Pages stay
 * left-aligned so the header doesn't move between pages of different widths.
 *
 * - `wide` (default): the layout's full content width, for list and detail pages.
 * - `narrow`: settings pages.
 * - `compact`: a page that is one small card, such as Invite.
 */
withDefaults(defineProps<{ width?: 'wide' | 'narrow' | 'compact' }>(), { width: 'wide' })
const widths = { wide: 'w-full', narrow: 'w-full max-w-3xl', compact: 'w-full max-w-md' }
</script>

<template>
  <div class="flex flex-col gap-8" :class="widths[width]"><slot /></div>
</template>
