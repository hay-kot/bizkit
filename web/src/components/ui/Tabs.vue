<script setup lang="ts">
import { useRoute, useRouter, type RouteLocationRaw } from 'vue-router'

export interface Tab {
  label: string
  to: RouteLocationRaw
  /** Active only on this exact path, for a tab whose path prefixes the others. */
  exact?: boolean
}

/**
 * A row of tabs that are links, one route each, so Back and shared links
 * work. A tab is active on its path and, unless `exact`, on paths under it
 * (a funnel's page keeps Funnels active). The line under the row spans the
 * full width, so it reads as the edge of the page header.
 */
defineProps<{ tabs: Tab[]; label: string }>()
const route = useRoute()
const router = useRouter()

function isActive(tab: Tab): boolean {
  const path = router.resolve(tab.to).path.replace(/\/$/, '')
  const current = route.path.replace(/\/$/, '')
  return current === path || (!tab.exact && current.startsWith(`${path}/`))
}
</script>

<template>
  <!-- The line is a pseudo-element on a wrapper that doesn't scroll, so it
  spans the row after a horizontal scroll, and it paints before the tabs, so
  the active tab's underline covers it. -->
  <div
    class="relative before:absolute before:inset-x-0 before:bottom-0 before:h-px before:bg-base-300"
  >
    <nav :aria-label="label" class="overflow-x-auto">
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
            class="tab whitespace-nowrap"
            :class="isActive(tab) ? 'tab-active' : ''"
            :aria-current="isActive(tab) ? 'page' : undefined"
            @click="navigate"
            >{{ tab.label }}</a
          >
        </RouterLink>
      </div>
    </nav>
  </div>
</template>
