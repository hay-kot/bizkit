<script setup lang="ts">
import { onClickOutside } from '@vueuse/core'
import { nextTick, ref, useId } from 'vue'

import { useFloatingList } from './floatingList'

/**
 * A small non-modal panel under its trigger for a short form (a filter):
 * teleported and placed like a `Select` list, on the `raised` surface.
 * The `trigger` slot gets `open`, `toggle` and `controls` (the panel's id,
 * for `aria-controls`); the default slot gets `close`. Focus moves to the
 * panel's first field on open and back to the trigger on close. Escape, a
 * click outside or tabbing away closes it; lists that open inside it
 * (`Select`, `Combobox`) count as inside. A scroll moves it with its
 * trigger rather than closing it.
 */
defineProps<{ label: string }>()
const emit = defineEmits<{ close: [] }>()
const open = ref(false)
const anchor = ref<HTMLElement>()
const panel = ref<HTMLElement>()
const id = useId()
const INSIDE = '[data-floating]'

const { position, update } = useFloatingList(anchor, panel, open, () => close(false), {
  follow: true,
})

async function show() {
  update()
  open.value = true
  await nextTick()
  panel.value
    ?.querySelector<HTMLElement>('[data-autofocus], input, [role="combobox"], button')
    ?.focus()
}

function close(refocus = true) {
  if (!open.value) return
  open.value = false
  emit('close')
  if (!refocus) return
  // After the render, so a trigger that the change re-rendered (an edited
  // filter chip) gets the focus, not <body>.
  void nextTick(() => anchor.value?.querySelector<HTMLElement>(`[aria-controls="${id}"]`)?.focus())
}

function toggle() {
  if (open.value) close()
  else void show()
}

onClickOutside(panel, () => close(false), { ignore: [anchor, INSIDE] })

function onKey(event: KeyboardEvent) {
  // An open list inside handles its own Escape and prevents the default.
  if (event.key === 'Escape' && !event.defaultPrevented) {
    event.preventDefault()
    close()
  }
}

function onFocusOut(event: FocusEvent) {
  const next = event.relatedTarget
  if (!(next instanceof Element)) return
  if (anchor.value?.contains(next) || next.closest(INSIDE)) return
  close(false)
}

defineExpose({ close })
</script>

<template>
  <span ref="anchor" class="inline-flex">
    <slot name="trigger" :open="open" :toggle="toggle" :controls="id" />
  </span>
  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-100 ease-out"
      enter-from-class="-translate-y-1 opacity-0"
      leave-active-class="transition duration-75 ease-in"
      leave-to-class="opacity-0"
    >
      <!-- eslint-disable vue/no-restricted-v-bind -- a floating panel needs coordinates from its trigger -->
      <div
        v-if="open"
        :id="id"
        ref="panel"
        data-floating
        role="dialog"
        :aria-label="label"
        :style="position"
        class="fixed z-50 w-80 rounded-box border border-base-300 bg-raised p-4 shadow-lg"
        @keydown="onKey"
        @focusout="onFocusOut"
      >
        <!-- eslint-enable vue/no-restricted-v-bind -->
        <slot :close="close" />
      </div>
    </Transition>
  </Teleport>
</template>
