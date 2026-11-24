<script setup lang="ts" generic="T extends string | number">
import { computed, ref } from 'vue'

export interface Segment<V extends string | number> {
  value: V
  label: string
  /** The name screen readers hear when the label is terse (`1/3`). */
  description?: string
}

/**
 * A few mutually exclusive choices side by side, as a radio group: one tab
 * stop, arrow keys move the choice. Use it for two to five short options
 * the user flips between; a longer list belongs in a `Select`.
 */
const props = withDefaults(
  defineProps<{
    options: Segment<T>[]
    label: string
    size?: 'sm' | 'toolbar'
    disabled?: boolean
  }>(),
  { size: 'toolbar', disabled: false },
)
const model = defineModel<T>({ required: true })
const heights = { sm: 'h-8', toolbar: 'h-9' }
const group = ref<HTMLElement>()
// With no choice made the first option takes the tab stop.
const tabStop = computed(() =>
  Math.max(
    0,
    props.options.findIndex((o) => o.value === model.value),
  ),
)

function pick(index: number) {
  const option = props.options[index]
  if (!option || props.disabled) return
  model.value = option.value
  // Template ref arrays don't keep v-for order, so find the button by index.
  group.value?.querySelector<HTMLElement>(`[data-index="${index}"]`)?.focus()
}

function onKey(event: KeyboardEvent, index: number) {
  const last = props.options.length - 1
  const next: Record<string, number> = {
    ArrowLeft: index === 0 ? last : index - 1,
    ArrowUp: index === 0 ? last : index - 1,
    ArrowRight: index === last ? 0 : index + 1,
    ArrowDown: index === last ? 0 : index + 1,
    Home: 0,
    End: last,
  }
  const to = next[event.key]
  if (to === undefined) return
  event.preventDefault()
  pick(to)
}
</script>

<template>
  <div
    ref="group"
    role="radiogroup"
    :aria-label="label"
    :aria-disabled="disabled || undefined"
    class="inline-flex items-center rounded-field border border-base-300 bg-base-200 p-0.5"
    :class="heights[size]"
  >
    <button
      v-for="(option, index) in options"
      :key="option.value"
      :data-index="index"
      type="button"
      :disabled="disabled"
      role="radio"
      :aria-checked="option.value === model"
      :aria-label="option.description"
      :tabindex="index === tabStop ? 0 : -1"
      class="h-full rounded-selector px-2.5 text-xs font-medium tabular-nums transition-colors focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-primary disabled:cursor-not-allowed disabled:opacity-60"
      :class="
        option.value === model
          ? 'bg-base-100 text-base-content shadow-sm'
          : 'text-base-content/70 hover:text-base-content'
      "
      @click="pick(index)"
      @keydown="onKey($event, index)"
    >
      {{ option.label }}
    </button>
  </div>
</template>
