<script setup lang="ts">
/**
 * An ordered vertical run of `TimelineItem`s joined by a connector line, for
 * things that happen one after another (a sequence's steps).
 */
defineProps<{ label?: string }>()
</script>

<template>
  <ol class="flex flex-col" :aria-label="label">
    <slot />
  </ol>
</template>
