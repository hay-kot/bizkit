<script setup lang="ts">
import CodeBlock from '@/components/ui/CodeBlock.vue'
import type { ApiEntry } from '@/queries/extensions'

defineProps<{ entries: ApiEntry[] }>()
</script>

<template>
  <ul class="flex flex-col divide-y divide-base-300" aria-label="Script API">
    <li v-for="entry in entries" :key="entry.name" class="py-3 first:pt-0 last:pb-0">
      <h4 class="font-mono text-sm font-semibold">{{ entry.name }}</h4>
      <CodeBlock class="mt-1" :code="entry.signature" language="typescript-type" wrap />
      <p class="mt-2 text-sm text-base-content/80">{{ entry.summary }}</p>
      <template v-if="entry.errors.length > 0">
        <p class="mt-2 text-xs font-medium">Throws when</p>
        <ul class="mt-1 list-disc pl-5 text-sm text-base-content/80">
          <li v-for="error in entry.errors" :key="error">{{ error }}</li>
        </ul>
      </template>
      <p class="mt-2 text-xs font-medium">Example</p>
      <CodeBlock class="mt-1" :code="entry.example" language="javascript" wrap />
    </li>
  </ul>
</template>
