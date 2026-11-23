<script setup lang="ts">
/**
 * Report panels two-up on wide screens and one column below, so report
 * pages get their grid without breaking the pages' one-column rule.
 */
</script>

<template>
  <div class="grid grid-cols-1 gap-8 lg:grid-cols-2"><slot /></div>
</template>
