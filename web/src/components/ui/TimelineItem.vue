<script setup lang="ts">
/**
 * One entry of a `Timeline`: a numbered marker in `primary` beside the
 * content, and a connector down to the next entry. The `chip` slot sits on
 * the connector above the entry, for what lies between it and the one before
 * (a delay).
 */
defineProps<{ number: number }>()

defineSlots<{
  default: () => unknown
  chip?: () => unknown
}>()
</script>

<template>
  <li class="group/timeline flex flex-col">
    <div v-if="$slots.chip" class="flex gap-4">
      <div class="flex w-8 shrink-0 justify-center">
        <span class="w-px bg-base-300 group-first/timeline:invisible" aria-hidden="true"></span>
      </div>
      <div class="py-2">
        <span
          class="inline-flex items-center gap-1.5 rounded-full border border-base-300 bg-base-100 px-2.5 py-0.5 text-xs text-base-content/70"
          ><slot name="chip"
        /></span>
      </div>
    </div>
    <div class="flex gap-4">
      <div class="flex w-8 shrink-0 flex-col items-center">
        <span
          class="inline-flex size-8 shrink-0 items-center justify-center rounded-full bg-primary text-sm font-semibold text-primary-content tabular-nums"
          aria-hidden="true"
          >{{ number }}</span
        >
        <span
          class="min-h-4 w-px flex-1 bg-base-300 group-last/timeline:invisible"
          aria-hidden="true"
        ></span>
      </div>
      <div class="min-w-0 flex-1 pb-2 group-last/timeline:pb-0"><slot /></div>
    </div>
  </li>
</template>
