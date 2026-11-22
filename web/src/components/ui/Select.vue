<script setup lang="ts" generic="T extends string">
import { onClickOutside, useElementBounding, useEventListener, useWindowSize } from '@vueuse/core'
import { Check, ChevronsUpDown } from 'lucide-vue-next'
import { computed, nextTick, ref, useId } from 'vue'

export interface SelectOption<V extends string> {
  value: V
  label: string
  description?: string
}

/**
 * A listbox select: the button shows the choice, the popup lists the options.
 * Arrow keys move, Enter or Space picks, Escape closes, typing jumps.
 */
const props = withDefaults(
  defineProps<{ options: SelectOption<T>[]; label: string; size?: 'sm' | 'md'; name?: string }>(),
  { size: 'md', name: undefined },
)
const model = defineModel<T>({ required: true })
const open = ref(false)
const active = ref(0)
const root = ref<HTMLElement>()
const button = ref<HTMLButtonElement>()
const list = ref<HTMLElement>()
const listId = useId()
const selected = computed(() => props.options.find((o) => o.value === model.value))

onClickOutside(root, () => (open.value = false), { ignore: [list] })

// The list renders in <body> so table and panel scroll containers can't clip
// it; it opens upward when there's no room below.
const anchor = useElementBounding(button)
const viewport = useWindowSize()
const position = computed(() => {
  const below = viewport.height.value - anchor.bottom.value
  const up = below < 272 && anchor.top.value > below
  return {
    left: `${anchor.left.value}px`,
    width: `${anchor.width.value}px`,
    ...(up
      ? { bottom: `${viewport.height.value - anchor.top.value + 4}px` }
      : { top: `${anchor.bottom.value + 4}px` }),
  }
})
useEventListener(
  'scroll',
  (event: Event) => {
    if (open.value && !list.value?.contains(event.target as Node)) open.value = false
  },
  { capture: true, passive: true },
)

async function show() {
  anchor.update()
  active.value = Math.max(
    0,
    props.options.findIndex((o) => o.value === model.value),
  )
  open.value = true
  await nextTick()
}

function pick(index: number) {
  const option = props.options[index]
  if (option) model.value = option.value
  open.value = false
  button.value?.focus()
}

function onKey(event: KeyboardEvent) {
  const last = props.options.length - 1
  if (!open.value) {
    if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key)) {
      event.preventDefault()
      void show()
    }
    return
  }
  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault()
      active.value = Math.min(last, active.value + 1)
      break
    case 'ArrowUp':
      event.preventDefault()
      active.value = Math.max(0, active.value - 1)
      break
    case 'Home':
      event.preventDefault()
      active.value = 0
      break
    case 'End':
      event.preventDefault()
      active.value = last
      break
    case 'Enter':
    case ' ':
      event.preventDefault()
      pick(active.value)
      break
    case 'Escape':
    case 'Tab':
      open.value = false
      break
    default:
      if (event.key.length === 1) {
        const index = props.options.findIndex((o) =>
          o.label.toLowerCase().startsWith(event.key.toLowerCase()),
        )
        if (index >= 0) active.value = index
      }
  }
}
</script>

<template>
  <div ref="root" class="relative">
    <button
      ref="button"
      type="button"
      role="combobox"
      :name="name"
      :aria-label="label"
      :aria-expanded="open"
      aria-haspopup="listbox"
      :aria-controls="listId"
      :aria-activedescendant="open ? `${listId}-${active}` : undefined"
      :data-value="model"
      class="flex w-full items-center justify-between gap-2 rounded-field border border-base-300 bg-base-100 text-left transition-colors hover:border-base-content/40 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-secondary"
      :class="size === 'sm' ? 'h-8 px-2.5 text-sm' : 'h-10 px-3'"
      @click="open ? (open = false) : show()"
      @keydown="onKey"
    >
      <span class="truncate">{{ selected?.label ?? 'Choose…' }}</span>
      <ChevronsUpDown class="size-4 shrink-0 text-base-content/70" aria-hidden="true" />
    </button>
    <Teleport to="body">
      <Transition
        enter-active-class="transition duration-100 ease-out"
        enter-from-class="-translate-y-1 opacity-0"
        leave-active-class="transition duration-75 ease-in"
        leave-to-class="opacity-0"
      >
        <!-- eslint-disable vue/no-restricted-v-bind -- a floating list needs coordinates from its button -->
        <ul
          v-if="open"
          :id="listId"
          ref="list"
          role="listbox"
          :aria-label="label"
          :style="position"
          class="fixed z-60 max-h-64 min-w-40 overflow-auto rounded-box border border-base-300 bg-base-100 p-1 shadow-lg"
        >
          <li
            v-for="(option, index) in options"
            :id="`${listId}-${index}`"
            :key="option.value"
            role="option"
            :aria-selected="option.value === model"
            class="flex cursor-pointer items-start gap-2 rounded-field px-2 py-1.5 text-sm"
            :class="index === active ? 'bg-base-200' : ''"
            @mouseenter="active = index"
            @mousedown.prevent
            @click="pick(index)"
          >
            <Check
              class="mt-0.5 size-4 shrink-0"
              :class="option.value === model ? 'opacity-100' : 'opacity-0'"
              aria-hidden="true"
            />
            <span class="flex flex-col">
              <span>{{ option.label }}</span>
              <span v-if="option.description" class="text-xs text-base-content/70">{{
                option.description
              }}</span>
            </span>
          </li>
        </ul>
        <!-- eslint-enable vue/no-restricted-v-bind -->
      </Transition>
    </Teleport>
  </div>
</template>
