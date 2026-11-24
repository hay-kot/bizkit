<script setup lang="ts">
import { useRoute, useRouter, type RouteLocationRaw } from 'vue-router'

import { formatNumber } from '@/lib/format'

export interface Tab {
  label: string
  to: RouteLocationRaw
  /** Active only on this exact path, for a tab whose path prefixes the others. */
  exact?: boolean
  /** A count shown after the label, such as the rows the tab lists. */
  count?: number
  /** More paths that keep the tab active, with the paths under them. */
  also?: string[]
}

/**
 * A row of tabs that are links, one route each, so Back and shared links
 * work. A tab is active on its path and, unless `exact`, on paths under it
 * (a funnel's page keeps Funnels active). The row's bottom line spans the
 * full width, so it reads as the edge of the page header.
 */
defineProps<{ tabs: Tab[]; label: string }>()
const route = useRoute()
const router = useRouter()

const trim = (path: string) => path.replace(/\/$/, '')

function isActive(tab: Tab): boolean {
  const current = trim(route.path)
  const path = trim(router.resolve(tab.to).path)
  if (current === path || (!tab.exact && current.startsWith(`${path}/`))) return true
  return (tab.also ?? []).some((p) => current === trim(p) || current.startsWith(`${trim(p)}/`))
}
</script>

<template>
  <!-- The line is a pseudo-element painted before the tabs, not a border,
  so the active tab's underline sits on top of it instead of above it. -->
  <nav
    :aria-label="label"
    class="relative overflow-x-auto before:absolute before:inset-x-0 before:bottom-0 before:h-px before:bg-base-300"
  >
    <div class="tabs-border tabs flex-nowrap">
      <RouterLink
        v-for="tab in tabs"
        :key="tab.label"
        v-slot="{ href, navigate }"
        :to="tab.to"
        custom
      >
        <a
          :href="href"
          class="tab gap-2 whitespace-nowrap"
          :class="isActive(tab) ? 'tab-active' : ''"
          :aria-current="isActive(tab) ? 'page' : undefined"
          @click="navigate"
        >
          {{ tab.label }}
          <span v-if="tab.count !== undefined" class="badge badge-ghost badge-sm">{{
            formatNumber(tab.count)
          }}</span>
        </a>
      </RouterLink>
    </div>
  </nav>
</template>
