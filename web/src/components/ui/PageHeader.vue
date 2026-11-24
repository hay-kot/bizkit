<script setup lang="ts">
import { computed, type Component } from 'vue'

import { usePageCrumbs, usePageRoute, type Crumb } from './pageHeader'

/**
 * The top of a page: a breadcrumb row, the module icon and title with the
 * page's actions on the same row, the description, and the `tabs` slot.
 *
 * `icon` and `crumbs` default to the route's module (its nav item) and the
 * workspace shell's root crumb, so most pages pass neither.
 */
const props = defineProps<{
  title: string
  description?: string
  icon?: Component
  crumbs?: Crumb[]
}>()
defineSlots<{ default?: () => unknown; tabs?: () => unknown }>()

const page = usePageRoute()
const defaults = usePageCrumbs(() => props.title)
const icon = computed(() => props.icon ?? page?.nav?.icon)
const trail = computed(() => {
  const crumbs = props.crumbs ?? defaults.value
  return crumbs.map((crumb, i) => (i === crumbs.length - 1 ? { label: crumb.label } : crumb))
})
</script>

<template>
  <header class="flex flex-col gap-2">
    <nav v-if="trail.length > 0" aria-label="Breadcrumb" class="min-w-0">
      <ol class="flex h-5 flex-wrap items-center gap-x-1.5 text-sm text-base-content/70">
        <li v-for="(crumb, i) in trail" :key="i" class="flex min-w-0 items-center gap-1.5">
          <span v-if="i > 0" aria-hidden="true">/</span>
          <span
            v-if="!crumb.label"
            class="h-3.5 w-20 animate-pulse rounded-field bg-base-300"
            aria-label="Loading"
            data-testid="crumb-skeleton"
          ></span>
          <RouterLink
            v-else-if="crumb.to"
            :to="crumb.to"
            class="truncate transition-colors hover:text-base-content"
            >{{ crumb.label }}</RouterLink
          >
          <span v-else aria-current="page" class="truncate">{{ crumb.label }}</span>
        </li>
      </ol>
    </nav>
    <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
      <div class="flex min-w-0 items-center gap-3">
        <span
          v-if="icon"
          class="flex size-8 shrink-0 items-center justify-center rounded-field bg-primary/10 text-primary"
          data-testid="page-icon"
        >
          <component :is="icon" class="size-5" aria-hidden="true" />
        </span>
        <h1 class="min-w-0 text-2xl font-semibold tracking-tight break-words">{{ title }}</h1>
      </div>
      <div v-if="$slots.default" class="flex flex-wrap items-center gap-2"><slot /></div>
    </div>
    <p v-if="description" class="max-w-prose text-sm text-base-content/80">{{ description }}</p>
    <div v-if="$slots.tabs" class="mt-2"><slot name="tabs" /></div>
  </header>
</template>
