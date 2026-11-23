<script setup lang="ts">
import { onClickOutside, onKeyStroke } from '@vueuse/core'
import { nextTick, ref, useId } from 'vue'

/**
 * A button that opens a menu of actions. Put `MenuItem`s (or any links and
 * buttons with role="menuitem") in the default slot. The trigger must set
 * `aria-controls` to the `controls` slot prop: the menu finds it by that to
 * return focus.
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
onKeyStroke('Escape', () => {
  if (!open.value) return
  open.value = false
  trigger()?.focus()
})

function trigger(): HTMLElement | null {
  return root.value?.querySelector<HTMLElement>(`[aria-controls="${id}"]`) ?? null
}

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
  if (open.value) void nextTick(() => items()[0]?.focus())
}

// Runs in the capture phase, before the item's own handler, so a confirm
// dialog the item opens records the trigger, not the item that is about to
// unmount, as the element to give focus back to.
function onPick() {
  trigger()?.focus()
  open.value = false
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
        @click.capture="onPick"
      >
        <slot />
      </div>
    </Transition>
  </div>
</template>
