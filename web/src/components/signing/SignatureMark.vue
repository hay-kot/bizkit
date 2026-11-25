<script setup lang="ts">
import { computed } from 'vue'

import { PAD_HEIGHT, PAD_WIDTH, type SigningSignature } from '@/queries/signing'

/** An adopted signature or initials, scaled into its box with the aspect ratio kept. */
const props = defineProps<{ mark: SigningSignature }>()

const paths = computed(() =>
  props.mark.kind === 'drawn'
    ? props.mark.strokes.map((stroke) =>
        stroke.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x} ${y}`).join(' '),
      )
    : [],
)
</script>

<template>
  <svg
    v-if="mark.kind === 'drawn'"
    class="size-full fill-none stroke-current"
    :viewBox="`0 0 ${PAD_WIDTH} ${PAD_HEIGHT}`"
    preserveAspectRatio="xMidYMid meet"
    stroke-width="8"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path v-for="(d, i) in paths" :key="i" :d="d" />
  </svg>
  <span v-else class="truncate font-serif text-lg italic">{{ mark.name }}</span>
</template>
