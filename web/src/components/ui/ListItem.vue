<script setup lang="ts">
import { computed } from 'vue'
import type { RouteLocationRaw } from 'vue-router'

import { stretchedTarget } from './stretchedTarget'

/**
 * One row of a `List`: leading visual, title, description, meta on the right
 * and trailing actions. With `to` the title is the row's one real link, with
 * `@select` its one button; either way its hit area stretches over the row,
 * and the trailing actions stay clickable above it.
 */
const props = withDefaults(
  defineProps<{
    title?: string
    description?: string
    /**
     * Lifts the description above the row's hit area, so its text can be
     * selected (a slug, an ID). Clicks on it don't open the row.
     */
    selectableDescription?: boolean
    to?: RouteLocationRaw
    /** The link's or button's accessible name, when the title alone repeats across rows. */
    label?: string
    /** Declared as a prop, not an emit, so the row knows whether it is clickable. */
    onSelect?: () => void
  }>(),
  {
    title: '',
    description: undefined,
    selectableDescription: false,
    to: undefined,
    label: undefined,
    onSelect: undefined,
  },
)

defineSlots<{
  leading?: () => unknown
  /** Replaces the `title` text, inside the link or button when there is one. */
  title?: () => unknown
  description?: () => unknown
  meta?: () => unknown
  trailing?: () => unknown
}>()

const clickable = computed(() => props.to !== undefined || props.onSelect !== undefined)
</script>

<template>
  <div
    class="flex items-center gap-3 px-4 py-3 group-first/list-item:rounded-t-box group-last/list-item:rounded-b-box"
    :class="clickable ? 'relative cursor-pointer transition-colors hover:bg-base-200' : ''"
  >
    <slot name="leading" />
    <div class="flex min-w-0 flex-1 flex-col gap-0.5">
      <RouterLink
        v-if="to !== undefined"
        :to="to"
        class="truncate"
        :class="stretchedTarget"
        :aria-label="label"
      >
        <slot name="title">{{ title }}</slot>
      </RouterLink>
      <button
        v-else-if="onSelect"
        type="button"
        class="cursor-pointer truncate text-start"
        :class="stretchedTarget"
        :aria-label="label"
        @click="onSelect()"
      >
        <slot name="title">{{ title }}</slot>
      </button>
      <span v-else class="truncate font-medium"
        ><slot name="title">{{ title }}</slot></span
      >
      <span
        v-if="description !== undefined || $slots.description"
        class="truncate text-sm text-base-content/70"
        :class="selectableDescription ? 'relative self-start' : ''"
        ><slot name="description">{{ description }}</slot></span
      >
    </div>
    <div v-if="$slots.meta" class="shrink-0 text-sm text-base-content/70">
      <slot name="meta" />
    </div>
    <div v-if="$slots.trailing" class="relative flex shrink-0 items-center gap-1">
      <slot name="trailing" />
    </div>
  </div>
</template>
