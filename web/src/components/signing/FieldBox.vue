<script setup lang="ts">
import { computed } from 'vue'

import type { Rect } from '@/lib/pdf'

/**
 * A box over a rendered PDF page at a field's rect, in page fractions. It is
 * the only positioned element on signing pages: put it in `PdfPages`'s
 * `#page` slot, and the content goes in the default slot.
 */
const props = defineProps<{ rect: Rect }>()

const percent = (fraction: number) => `${(fraction * 100).toFixed(4)}%`
const position = computed(() => ({
  left: percent(props.rect.x),
  top: percent(props.rect.y),
  width: percent(props.rect.width),
  height: percent(props.rect.height),
}))
</script>

<template>
  <!-- eslint-disable vue/no-restricted-v-bind -- a field sits at its own rect on the page, which no class can express -->
  <div class="absolute" :style="position">
    <slot />
  </div>
  <!-- eslint-enable vue/no-restricted-v-bind -->
</template>
