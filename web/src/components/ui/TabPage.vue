<script setup lang="ts">
import { inject, watchEffect } from 'vue'

import { TAB_LAYOUT } from './tabLayout'

/**
 * One tab's page inside a `TabLayout`: it puts its title, description,
 * width and `actions` slot in the layout's header, and renders its content
 * below the tabs.
 */
const props = withDefaults(
  defineProps<{ title: string; description?: string; width?: 'wide' | 'narrow' }>(),
  { description: undefined, width: 'wide' },
)
defineSlots<{ default?: () => unknown; actions?: () => unknown }>()

const layout = inject(TAB_LAYOUT)
if (!layout) throw new Error('TabPage renders only inside a TabLayout route')
watchEffect(() => {
  layout.header.title = props.title
  layout.header.description = props.description
  layout.header.width = props.width
})
</script>

<template>
  <div class="flex flex-col gap-8">
    <Teleport v-if="layout.actions.value && $slots.actions" :to="layout.actions.value">
      <slot name="actions" />
    </Teleport>
    <slot />
  </div>
</template>
