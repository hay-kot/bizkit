<script setup lang="ts">
import { CircleAlert, CircleCheck, Info, X } from 'lucide-vue-next'

import { dismissToast, toasts } from '@/lib/toast'

const icons = { success: CircleCheck, error: CircleAlert, info: Info }
const tone = { success: 'text-success', error: 'text-error', info: 'text-info' }
</script>

<template>
  <div
    class="pointer-events-none fixed right-4 bottom-4 z-50 flex w-80 max-w-full flex-col gap-2"
    aria-live="polite"
  >
    <TransitionGroup
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="translate-y-2 opacity-0"
      leave-active-class="transition duration-150 ease-in"
      leave-to-class="opacity-0"
      move-class="transition duration-200"
    >
      <div
        v-for="item in toasts"
        :key="item.id"
        role="status"
        class="pointer-events-auto flex items-start gap-3 rounded-box border border-base-300 bg-base-100 p-3 text-sm shadow-lg"
      >
        <component
          :is="icons[item.kind]"
          class="mt-0.5 size-4 shrink-0"
          :class="tone[item.kind]"
          aria-hidden="true"
        />
        <span class="flex-1">{{ item.message }}</span>
        <button
          type="button"
          class="btn btn-square btn-ghost btn-xs"
          aria-label="Dismiss"
          @click="dismissToast(item.id)"
        >
          <X class="size-3.5" aria-hidden="true" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>
