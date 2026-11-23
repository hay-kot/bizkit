<script setup lang="ts">
import BrandMark from '@/components/BrandMark.vue'
</script>

<template>
  <div
    class="relative isolate flex min-h-screen flex-col items-center justify-center auth-backdrop px-4 py-12"
  >
    <BrandMark class="mb-8 text-lg" />
    <div class="w-full max-w-sm">
      <RouterView v-slot="{ Component }">
        <Transition
          mode="out-in"
          enter-active-class="transition duration-150 ease-out"
          enter-from-class="translate-y-1 opacity-0"
          leave-active-class="transition duration-100 ease-in"
          leave-to-class="opacity-0"
        >
          <component :is="Component" />
        </Transition>
      </RouterView>
    </div>
  </div>
</template>
