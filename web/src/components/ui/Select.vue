<script setup lang="ts" generic="T extends string">
import { onClickOutside } from '@vueuse/core'
import { Check, ChevronsUpDown } from 'lucide-vue-next'
import { computed, nextTick, ref, useId } from 'vue'

import { useFloatingList } from './floatingList'

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
  defineProps<{
    options: SelectOption<T>[]
    label: string
    size?: 'sm' | 'toolbar' | 'md'
    name?: string
  }>(),
  { size: 'md', name: undefined },
)
const model = defineModel<T>({ required: true })
// sm fits a table row or an inline editor, toolbar matches the other 36px
// toolbar controls, md is a form control.
const heights = { sm: 'h-8', toolbar: 'h-9', md: 'h-10' }
const open = ref(false)
const active = ref(0)
const root = ref<HTMLElement>()
const button = ref<HTMLButtonElement>()
const list = ref<HTMLElement>()
const listId = useId()
const selected = computed(() => props.options.find((o) => o.value === model.value))

onClickOutside(root, () => (open.value = false), { ignore: [list] })

const { position, update } = useFloatingList(button, list, open)

async function show() {
  update()
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
      // The open list owns this Escape, so a popover or dialog around the
      // select stays open.
      event.preventDefault()
      event.stopPropagation()
      open.value = false
      break
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
      class="flex w-full items-center justify-between gap-2 rounded-field border border-field-border bg-field px-3 text-left text-sm transition-colors hover:border-base-content/60 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-primary"
      :class="heights[size]"
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
          data-floating
          role="listbox"
          :aria-label="label"
          :style="position"
          class="fixed z-60 max-h-80 overflow-auto rounded-box border border-base-300 bg-raised p-1.5 shadow-lg"
        >
          <li
            v-for="(option, index) in options"
            :id="`${listId}-${index}`"
            :key="option.value"
            role="option"
            :aria-selected="option.value === model"
            class="flex min-h-9 cursor-pointer items-center gap-2.5 rounded-field px-3 py-2 text-sm"
            :class="[
              index === active ? 'bg-base-content/5' : '',
              option.value === model ? 'font-medium text-primary' : '',
            ]"
            @mouseenter="active = index"
            @mousedown.prevent
            @click="pick(index)"
          >
            <span class="flex flex-1 flex-col">
              <span>{{ option.label }}</span>
              <span v-if="option.description" class="text-xs font-normal text-base-content/70">{{
                option.description
              }}</span>
            </span>
            <Check v-if="option.value === model" class="size-4 shrink-0" aria-hidden="true" />
          </li>
        </ul>
        <!-- eslint-enable vue/no-restricted-v-bind -->
      </Transition>
    </Teleport>
  </div>
</template>
