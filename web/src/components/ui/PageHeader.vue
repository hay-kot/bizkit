<script setup lang="ts">
import { ExternalLink } from 'lucide-vue-next'
import { computed, inject, type Component } from 'vue'
import { routeLocationKey } from 'vue-router'

import { defaultCrumbs, PAGE_CONTEXT, type Crumb } from './pageHeader'

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
  /** A docs URL, linked after the description. */
  docs?: string
}>()
defineSlots<{ default?: () => unknown; tabs?: () => unknown }>()

// Optional so the header renders outside a router, in unit tests.
const route = inject(routeLocationKey, undefined)
const context = inject(PAGE_CONTEXT, undefined)

const icon = computed(() => props.icon ?? route?.meta.nav?.icon)
const trail = computed(() => {
  const crumbs = props.crumbs ?? defaultCrumbs(context, route?.meta.nav, props.title)
  return crumbs.map((crumb, i) => (i === crumbs.length - 1 ? { label: crumb.label } : crumb))
})
</script>

<template>
  <header class="flex flex-col gap-2">
    <nav v-if="trail.length > 1" aria-label="Breadcrumb" class="min-w-0">
      <ol class="flex flex-wrap items-center gap-x-1.5 text-sm text-base-content/70">
        <li v-for="(crumb, i) in trail" :key="i" class="flex min-w-0 items-center gap-1.5">
          <span v-if="i > 0" aria-hidden="true">/</span>
          <RouterLink
            v-if="crumb.to"
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
    <p v-if="description || docs" class="max-w-prose text-sm text-base-content/80">
      {{ description }}
      <a
        v-if="docs"
        :href="docs"
        target="_blank"
        rel="noopener"
        class="inline-flex items-center gap-1 text-primary hover:underline"
        >Documentation<ExternalLink class="size-3.5" aria-hidden="true"
      /></a>
    </p>
    <div v-if="$slots.tabs" class="mt-2"><slot name="tabs" /></div>
  </header>
</template>
