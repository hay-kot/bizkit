<script setup lang="ts">
import { onClickOutside } from '@vueuse/core'
import { ChevronsUpDown } from 'lucide-vue-next'
import { computed, onBeforeUnmount, ref, useId, watch } from 'vue'

import { formatNumber } from '@/lib/format'

import { useFloatingList } from './floatingList'

export interface ComboOption {
  value: string
  label?: string
  description?: string
  count?: number
  group?: string
}

/** Looks options up for what was typed; the empty string asks for the defaults. */
export type ComboLookup = (query: string) => Promise<ComboOption[]>

/**
 * A typeahead: the text field filters the options and the popup lists them,
 * grouped, with counts. Arrow keys move, Enter picks, Escape closes. With
 * `allowCustom` a "Use ..." row takes text that isn't listed: typing puts the
 * highlight on it, so Enter keeps the typed text, and leaving the field keeps
 * it too.
 */
const props = withDefaults(
  defineProps<{
    options: ComboOption[] | ComboLookup
    label: string
    allowCustom?: boolean
    placeholder?: string
    size?: 'sm' | 'toolbar' | 'md'
    name?: string
    required?: boolean
  }>(),
  { allowCustom: false, placeholder: undefined, size: 'md', name: undefined, required: false },
)
const model = defineModel<string>({ required: true })

const DEBOUNCE_MS = 150
const heights = { sm: 'h-8', toolbar: 'h-9', md: 'h-10' }
const open = ref(false)
const active = ref(0)
const root = ref<HTMLElement>()
const input = ref<HTMLInputElement>()
const list = ref<HTMLElement>()
const listId = useId()

const looked = ref<ComboOption[]>([])
const looking = ref(false)
/** Labels of picked options, so an async pick still shows its label after the list changes. */
const picked = ref<ComboOption>()

function labelOf(option: ComboOption): string {
  return option.label ?? option.value
}

function display(value: string): string {
  const known = Array.isArray(props.options)
    ? props.options.find((o) => o.value === value)
    : [picked.value, ...looked.value].find((o) => o?.value === value)
  return known ? labelOf(known) : value
}

const text = ref(display(model.value))
/** True once the user types, until a pick or close; until then every option shows. */
const typed = ref(false)
const query = computed(() => (typed.value ? text.value.trim() : ''))

watch(model, (value) => {
  if (!typed.value) text.value = display(value)
})
watch(
  () => props.options,
  () => {
    if (!typed.value) text.value = display(model.value)
  },
)

const shown = computed((): ComboOption[] => {
  if (!Array.isArray(props.options)) return looked.value
  const needle = query.value.toLowerCase()
  if (!needle) return props.options
  return props.options.filter(
    (o) => labelOf(o).toLowerCase().includes(needle) || o.value.toLowerCase().includes(needle),
  )
})

type Row = { kind: 'option'; option: ComboOption } | { kind: 'custom'; value: string }
interface Section {
  group?: string
  id: string
  rows: { row: Row; index: number }[]
}

/**
 * Options under their group headings, in the order each group first appears,
 * then the custom row. `index` counts in display order, which is the order
 * the arrow keys walk.
 */
const sections = computed((): Section[] => {
  const out: Section[] = []
  for (const option of shown.value) {
    let section = out.find((s) => s.group === option.group)
    if (!section) {
      section = { group: option.group, id: `${listId}-g${out.length}`, rows: [] }
      out.push(section)
    }
    section.rows.push({ row: { kind: 'option', option }, index: 0 })
  }
  const custom = query.value
  if (props.allowCustom && custom && !shown.value.some((o) => o.value === custom)) {
    out.push({
      id: `${listId}-custom`,
      rows: [{ row: { kind: 'custom', value: custom }, index: 0 }],
    })
  }
  let index = 0
  for (const section of out) for (const entry of section.rows) entry.index = index++
  return out
})
const ordered = computed(() => sections.value.flatMap((s) => s.rows.map((r) => r.row)))

/**
 * Where the highlight starts after typing: on the custom row when there is
 * one, so Enter keeps the typed text rather than the first partial match.
 */
function firstActive(): number {
  const custom = ordered.value.findIndex((r) => r.kind === 'custom')
  return Math.max(0, custom)
}

watch(ordered, (rows) => {
  active.value = Math.max(0, Math.min(active.value, rows.length - 1))
})

let timer: ReturnType<typeof setTimeout> | undefined
let request = 0
function lookup(now = false) {
  const options = props.options
  if (Array.isArray(options)) return
  clearTimeout(timer)
  const run = async () => {
    const mine = ++request
    looking.value = true
    try {
      const found = await options(query.value)
      if (mine === request) looked.value = found
    } catch {
      if (mine === request) looked.value = []
    } finally {
      if (mine === request) looking.value = false
    }
  }
  if (now) void run()
  else timer = setTimeout(() => void run(), DEBOUNCE_MS)
}
watch(query, () => {
  if (open.value) lookup()
})
onBeforeUnmount(() => clearTimeout(timer))

const { position, update } = useFloatingList(input, list, open, () => close())

function show() {
  if (open.value) return
  update()
  const at = ordered.value.findIndex((r) => r.kind === 'option' && r.option.value === model.value)
  active.value = Math.max(0, at)
  open.value = true
  lookup(true)
}

function close() {
  open.value = false
  typed.value = false
  text.value = display(model.value)
}

function pick(row: Row | undefined) {
  if (!row) return
  if (row.kind === 'option') {
    picked.value = row.option
    model.value = row.option.value
  } else {
    model.value = row.value
  }
  close()
}

function onInput() {
  typed.value = true
  if (!open.value) show()
  active.value = firstActive()
}

function onBlur() {
  if (props.allowCustom && typed.value && query.value && query.value !== model.value) {
    model.value = query.value
  }
  close()
}

onClickOutside(root, () => open.value && close(), { ignore: [list] })

function onKey(event: KeyboardEvent) {
  const last = Math.max(0, ordered.value.length - 1)
  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault()
      if (!open.value) show()
      else active.value = Math.min(last, active.value + 1)
      break
    case 'ArrowUp':
      event.preventDefault()
      if (!open.value) show()
      else active.value = Math.max(0, active.value - 1)
      break
    case 'Enter':
      if (open.value) {
        event.preventDefault()
        pick(ordered.value[active.value])
      }
      break
    case 'Escape':
      // An open list owns this Escape: the overlay around the field must
      // stay open, and Overlay skips Escapes that are defaultPrevented.
      if (open.value) {
        event.preventDefault()
        event.stopPropagation()
        close()
      }
      break
  }
}

function toggle() {
  if (open.value) {
    close()
  } else {
    input.value?.focus()
    show()
  }
}

const loading = computed(() => looking.value && ordered.value.length === 0)
const empty = computed(() => !looking.value && ordered.value.length === 0)
</script>

<template>
  <div ref="root" class="relative">
    <input
      ref="input"
      v-model="text"
      type="text"
      role="combobox"
      autocomplete="off"
      spellcheck="false"
      :name="name"
      :required="required"
      :placeholder="placeholder"
      :aria-label="label"
      aria-autocomplete="list"
      :aria-expanded="open"
      :aria-controls="listId"
      :aria-activedescendant="open && ordered.length ? `${listId}-${active}` : undefined"
      class="w-full rounded-field border border-field-border bg-field pr-9 pl-3 text-sm transition-colors placeholder:text-base-content/50 hover:border-base-content/60 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-primary"
      :class="heights[size]"
      @input="onInput"
      @click="show"
      @keydown="onKey"
      @blur="onBlur"
    />
    <button
      type="button"
      tabindex="-1"
      :aria-label="`Show ${label} options`"
      class="absolute inset-y-0 right-0 flex w-9 items-center justify-center text-base-content/70"
      @mousedown.prevent
      @click="toggle"
    >
      <ChevronsUpDown class="size-4" aria-hidden="true" />
    </button>
    <Teleport to="body">
      <Transition
        enter-active-class="transition duration-100 ease-out"
        enter-from-class="-translate-y-1 opacity-0"
        leave-active-class="transition duration-75 ease-in"
        leave-to-class="opacity-0"
      >
        <!-- eslint-disable vue/no-restricted-v-bind -- a floating list needs coordinates from its field -->
        <div
          v-if="open"
          ref="list"
          :style="position"
          class="fixed z-60 max-h-80 overflow-auto rounded-box border border-base-300 bg-raised p-1.5 shadow-lg"
        >
          <!-- eslint-enable vue/no-restricted-v-bind -->
          <ul :id="listId" role="listbox" :aria-label="label">
            <li v-for="section in sections" :key="section.id" role="presentation">
              <div
                v-if="section.group"
                :id="section.id"
                class="px-3 pt-2 pb-1 text-xs font-medium text-base-content/60"
              >
                {{ section.group }}
              </div>
              <ul role="group" :aria-labelledby="section.group ? section.id : undefined">
                <li
                  v-for="{ row, index } in section.rows"
                  :id="`${listId}-${index}`"
                  :key="row.kind === 'option' ? `o:${row.option.value}` : 'custom'"
                  role="option"
                  :aria-selected="row.kind === 'option' && row.option.value === model"
                  class="flex min-h-9 cursor-pointer items-center gap-3 rounded-field px-3 py-2 text-sm"
                  :class="[
                    index === active ? 'bg-base-content/5' : '',
                    row.kind === 'option' && row.option.value === model
                      ? 'font-medium text-primary'
                      : '',
                  ]"
                  @mouseenter="active = index"
                  @mousedown.prevent
                  @click="pick(row)"
                >
                  <template v-if="row.kind === 'option'">
                    <span class="flex min-w-0 flex-1 flex-col">
                      <span class="truncate">{{ labelOf(row.option) }}</span>
                      <span
                        v-if="row.option.description"
                        class="text-xs font-normal text-base-content/70"
                        >{{ row.option.description }}</span
                      >
                    </span>
                    <span
                      v-if="row.option.count !== undefined"
                      class="shrink-0 text-xs font-normal text-base-content/60 tabular-nums"
                      >{{ formatNumber(row.option.count) }}</span
                    >
                  </template>
                  <span v-else class="truncate">Use “{{ row.value }}”</span>
                </li>
              </ul>
            </li>
          </ul>
          <p v-if="loading" class="px-3 py-2 text-sm text-base-content/70" role="status">
            Searching…
          </p>
          <p v-else-if="empty" class="px-3 py-2 text-sm text-base-content/70" role="status">
            No matches
          </p>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>
