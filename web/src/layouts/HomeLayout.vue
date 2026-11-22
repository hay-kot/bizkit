<script setup lang="ts">
import BrandMark from '@/components/BrandMark.vue'
import UserMenu from '@/components/UserMenu.vue'
</script>

<template>
  <div class="min-h-screen bg-base-200">
    <header class="border-b border-base-300 bg-base-100">
      <div class="mx-auto flex h-14 max-w-5xl items-center justify-between px-4 sm:px-6">
        <RouterLink to="/"><BrandMark /></RouterLink>
        <div class="w-56 max-w-1/2"><UserMenu /></div>
      </div>
    </header>
    <main class="mx-auto max-w-5xl px-4 py-8 sm:px-6">
      <RouterView v-slot="{ Component, route }">
        <Transition
          mode="out-in"
          enter-active-class="transition duration-150 ease-out"
          enter-from-class="translate-y-1 opacity-0"
          leave-active-class="transition duration-100 ease-in"
          leave-to-class="opacity-0"
        >
          <component :is="Component" :key="route.path" />
        </Transition>
      </RouterView>
    </main>
  </div>
</template>
