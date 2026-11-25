<script setup lang="ts">
import BrandMark from '@/components/BrandMark.vue'
</script>

<template>
  <!-- Pages for people without an account, such as a signer's link: no
       sidebar, and a column as wide as a document reads well. -->
  <div class="min-h-screen bg-base-200 px-4 py-8">
    <div class="mx-auto flex w-full max-w-4xl flex-col gap-8">
      <BrandMark class="text-lg" />
      <main>
        <RouterView />
      </main>
    </div>
  </div>
</template>
