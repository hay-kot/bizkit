<script setup lang="ts">
import { onClickOutside, onKeyStroke } from '@vueuse/core'
import { ref, useId } from 'vue'

/**
 * A button that opens a menu of actions. Put `MenuItem`s (or any links and
 * buttons with role="menuitem") in the default slot.
 */
withDefaults(
  defineProps<{ label: string; align?: 'start' | 'end'; placement?: 'below' | 'above' }>(),
  {
    align: 'start',
    placement: 'below',
  },
)
const open = ref(false)
const root = ref<HTMLElement>()
const menu = ref<HTMLElement>()
const id = useId()

onClickOutside(root, () => (open.value = false))
onKeyStroke('Escape', () => (open.value = false))

function items(): HTMLElement[] {
  return [...(menu.value?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])]
}

function onMenuKey(event: KeyboardEvent) {
  const list = items()
  const index = list.indexOf(document.activeElement as HTMLElement)
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    list[(index + 1) % list.length]?.focus()
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    list[(index - 1 + list.length) % list.length]?.focus()
  }
}

function toggle() {
  open.value = !open.value
}
</script>

<template>
  <div ref="root" class="relative">
    <slot name="trigger" :open="open" :toggle="toggle" :controls="id" />
    <Transition
      enter-active-class="transition duration-100 ease-out"
      :enter-from-class="
        placement === 'below' ? '-translate-y-1 opacity-0' : 'translate-y-1 opacity-0'
      "
      leave-active-class="transition duration-75 ease-in"
      leave-to-class="opacity-0"
    >
      <div
        v-if="open"
        :id="id"
        ref="menu"
        role="menu"
        :aria-label="label"
        class="absolute z-30 min-w-56 rounded-box border border-base-300 bg-base-100 p-1 shadow-lg"
        :class="[
          align === 'end' ? 'right-0' : 'left-0',
          placement === 'below' ? 'top-full mt-1' : 'bottom-full mb-1',
        ]"
        @keydown="onMenuKey"
        @click="open = false"
      >
        <slot />
      </div>
    </Transition>
  </div>
</template>
