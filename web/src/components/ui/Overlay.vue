<script setup lang="ts">
import { onKeyStroke, useScrollLock } from '@vueuse/core'
import { X } from 'lucide-vue-next'
import { nextTick, ref, useId, watch } from 'vue'

/**
 * A modal dialog (`variant="dialog"`, centered) or slide-over panel
 * (`variant="panel"`, from the right). Escape and the backdrop close it.
 */
const props = withDefaults(
  defineProps<{
    open: boolean
    title: string
    description?: string
    variant?: 'dialog' | 'panel'
  }>(),
  {
    variant: 'dialog',
    description: undefined,
  },
)
const emit = defineEmits<{ close: [] }>()
const titleId = useId()
const surface = ref<HTMLElement>()
const lock = useScrollLock(typeof document === 'undefined' ? null : document.body)
let restoreFocus: HTMLElement | null = null

watch(
  () => props.open,
  async (open) => {
    lock.value = open
    if (open) {
      restoreFocus = document.activeElement as HTMLElement | null
      await nextTick()
      const target =
        surface.value?.querySelector<HTMLElement>('[data-autofocus]') ??
        surface.value?.querySelector<HTMLElement>('input, textarea, [role="combobox"]') ??
        surface.value
      target?.focus()
    } else {
      restoreFocus?.focus()
    }
  },
)

onKeyStroke('Escape', () => {
  if (props.open) emit('close')
})

function trapFocus(event: KeyboardEvent) {
  if (event.key !== 'Tab' || !surface.value) return
  const focusable = [
    ...surface.value.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), input:not([disabled]), select, textarea, [tabindex]:not([tabindex="-1"])',
    ),
  ]
  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  if (!first || !last) return
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault()
    last.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first.focus()
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="opacity-0"
      leave-active-class="transition duration-150 ease-in"
      leave-to-class="opacity-0"
    >
      <div
        v-if="open"
        class="fixed inset-0 z-40 bg-neutral/40"
        aria-hidden="true"
        @click="emit('close')"
      ></div>
    </Transition>
    <Transition
      enter-active-class="transition duration-200 ease-out"
      :enter-from-class="variant === 'panel' ? 'translate-x-full' : 'scale-95 opacity-0'"
      leave-active-class="transition duration-150 ease-in"
      :leave-to-class="variant === 'panel' ? 'translate-x-full' : 'scale-95 opacity-0'"
    >
      <div
        v-if="open"
        class="pointer-events-none fixed inset-0 z-50 flex"
        :class="variant === 'panel' ? 'justify-end' : 'items-center justify-center p-4'"
      >
        <div
          ref="surface"
          role="dialog"
          aria-modal="true"
          tabindex="-1"
          :aria-labelledby="titleId"
          class="pointer-events-auto flex max-h-full flex-col bg-base-100 shadow-xl outline-none"
          :class="
            variant === 'panel'
              ? 'h-full w-full max-w-md border-l border-base-300'
              : 'w-full max-w-md rounded-box'
          "
          @keydown="trapFocus"
        >
          <header class="flex items-start justify-between gap-4 border-b border-base-300 px-5 py-4">
            <div>
              <h2 :id="titleId" class="text-base font-semibold">{{ title }}</h2>
              <p v-if="description" class="mt-0.5 text-sm text-base-content/80">
                {{ description }}
              </p>
            </div>
            <button
              type="button"
              class="btn btn-square btn-ghost btn-sm"
              aria-label="Close"
              @click="emit('close')"
            >
              <X class="size-4" aria-hidden="true" />
            </button>
          </header>
          <div class="flex-1 overflow-y-auto px-5 py-4">
            <slot />
          </div>
          <footer
            v-if="$slots.footer"
            class="flex justify-end gap-2 border-t border-base-300 px-5 py-3"
          >
            <slot name="footer" />
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
