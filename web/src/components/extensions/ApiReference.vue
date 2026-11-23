<script setup lang="ts">
import type { ApiEntry } from '@/queries/extensions'

defineProps<{ entries: ApiEntry[] }>()
</script>

<template>
  <ul class="flex flex-col divide-y divide-base-300" aria-label="Script API">
    <li v-for="entry in entries" :key="entry.name" class="py-3 first:pt-0 last:pb-0">
      <h4 class="font-mono text-sm font-semibold">{{ entry.name }}</h4>
      <pre
        tabindex="0"
        class="mt-1 overflow-x-auto rounded-field bg-base-200 p-2 text-xs whitespace-pre-wrap"
      ><code>{{ entry.signature }}</code></pre>
      <p class="mt-2 text-sm text-base-content/80">{{ entry.summary }}</p>
      <template v-if="entry.errors.length > 0">
        <p class="mt-2 text-xs font-medium">Throws when</p>
        <ul class="mt-1 list-disc pl-5 text-sm text-base-content/80">
          <li v-for="error in entry.errors" :key="error">{{ error }}</li>
        </ul>
      </template>
      <p class="mt-2 text-xs font-medium">Example</p>
      <pre
        tabindex="0"
        class="mt-1 overflow-x-auto rounded-field bg-base-200 p-2 text-xs whitespace-pre-wrap"
      ><code>{{ entry.example }}</code></pre>
    </li>
  </ul>
</template>
