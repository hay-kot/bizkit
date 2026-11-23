<script setup lang="ts">
import { useId } from 'vue'

import SectionHeading from './SectionHeading.vue'

/**
 * A titled block on a page that isn't a `Card`, usually a table or a list.
 * The `actions` slot goes to the heading, the default slot under it.
 */
defineProps<{ title: string; description?: string }>()
const headingId = useId()
</script>

<template>
  <section class="flex flex-col gap-3" :aria-labelledby="headingId">
    <SectionHeading :id="headingId" :title="title" :description="description">
      <template v-if="$slots.description" #description><slot name="description" /></template>
      <template v-if="$slots.actions" #actions><slot name="actions" /></template>
    </SectionHeading>
    <slot />
  </section>
</template>
