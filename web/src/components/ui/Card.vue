<script setup lang="ts">
import { useId } from 'vue'

import SectionHeading from './SectionHeading.vue'

/**
 * A bordered section of a page with a `SectionHeading` on top. The
 * `actions` slot goes to the heading, the default slot to the body, and
 * the `footer` slot to a right-aligned row under the body (a settings
 * card's Save).
 */
defineProps<{ title: string; description?: string }>()
const headingId = useId()
</script>

<template>
  <section
    class="flex flex-col rounded-box border border-base-300 bg-base-100"
    :aria-labelledby="headingId"
  >
    <div class="border-b border-base-300 px-4 py-3">
      <SectionHeading :id="headingId" :title="title" :description="description">
        <template v-if="$slots.description" #description><slot name="description" /></template>
        <template v-if="$slots.actions" #actions><slot name="actions" /></template>
      </SectionHeading>
    </div>
    <div v-if="$slots.default" class="flex flex-1 flex-col gap-4 p-4"><slot /></div>
    <div
      v-if="$slots.footer"
      class="flex items-center justify-end gap-2 border-base-300 px-4 py-3"
      :class="{ 'border-t': $slots.default }"
    >
      <slot name="footer" />
    </div>
  </section>
</template>
