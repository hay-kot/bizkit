<script setup lang="ts">
import { inject, onBeforeUnmount, useSlots, watchEffect } from 'vue'

import { TAB_LAYOUT } from './tabLayout'

/**
 * One tab's page inside a `TabLayout`: it puts its title, description and
 * `actions` slot in the layout's header, and renders its content below the
 * tabs.
 */
const props = withDefaults(defineProps<{ title: string; description?: string }>(), {
  description: undefined,
})
defineSlots<{ default?: () => unknown; actions?: () => unknown }>()

const layout = inject(TAB_LAYOUT)
if (!layout) throw new Error('TabPage renders only inside a TabLayout route')
const hasActions = Boolean(useSlots().actions)
if (hasActions) layout.withActions.value += 1
onBeforeUnmount(() => {
  if (hasActions) layout.withActions.value -= 1
})
watchEffect(() => {
  layout.header.title = props.title
  layout.header.description = props.description
})
</script>

<template>
  <div class="flex flex-col gap-8">
    <Teleport
      v-if="hasActions && layout.actions.value && !layout.leaving.value"
      :to="layout.actions.value"
    >
      <slot name="actions" />
    </Teleport>
    <slot />
  </div>
</template>
