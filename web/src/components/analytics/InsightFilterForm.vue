<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import Combobox, { type ComboOption } from '@/components/ui/Combobox.vue'
import Select from '@/components/ui/Select.vue'
import type { DayRange } from '@/lib/range'
import {
  type FilterOp,
  type InsightKind,
  useSchemaKeys,
  useSchemaValues,
} from '@/queries/analytics'

import { type FilterDraft, isComplete, opsFor } from './insightDraft'

/**
 * One filter in the builder's popover: the key from the schema (event
 * properties of the chosen event, or person traits), a comparison that
 * fits the key's types, and a value from the key's top values or typed.
 */
const props = defineProps<{
  workspaceId: string
  projectId: string
  range: DayRange
  kind: InsightKind
  event: string
  initial: FilterDraft
  applyLabel: string
}>()
const emit = defineEmits<{ apply: [filter: FilterDraft]; cancel: [] }>()

const key = ref(props.initial.key)
const op = ref<FilterOp>(props.initial.op)
const value = ref(props.initial.value)

const source = computed(() => (props.kind === 'people' ? 'trait' : 'property'))
// Traits describe people now, so their lookups ignore the event and range.
const scope = computed(() =>
  source.value === 'trait'
    ? {}
    : { event: props.event || undefined, from: props.range.from, to: props.range.to },
)
const keys = useSchemaKeys(
  () => props.workspaceId,
  () => props.projectId,
  () => ({ source: source.value, limit: 100, ...scope.value }),
)
const values = useSchemaValues(
  () => props.workspaceId,
  () => props.projectId,
  () => ({ source: source.value, key: key.value.trim(), ...scope.value }),
)

const keyOptions = computed((): ComboOption[] => {
  const items = keys.data.value?.items ?? []
  const options: ComboOption[] = items.map((k) => ({
    value: k.key,
    count: k.count,
    description: k.types.join(', '),
  }))
  if (key.value && !items.some((k) => k.key === key.value)) options.unshift({ value: key.value })
  return options
})
const keyTypes = computed(() => keys.data.value?.items.find((k) => k.key === key.value)?.types)
const ops = computed(() => opsFor(keyTypes.value))
watch(ops, (allowed) => {
  if (!allowed.some((o) => o.value === op.value)) op.value = 'is'
})
const valueOptions = computed((): ComboOption[] =>
  (values.data.value?.items ?? []).map((v) => ({ value: v.value, count: v.count })),
)

const draft = computed((): FilterDraft => ({ key: key.value, op: op.value, value: value.value }))
const numeric = computed(() => op.value === 'gt' || op.value === 'lt')
const complete = computed(() => isComplete(draft.value))
const caption = 'text-xs font-medium text-base-content/70'
const what = computed(() => (props.kind === 'people' ? 'Trait' : 'Property'))

// Apply stays enabled: a value typed but not yet committed lands on blur,
// which runs on the button's mousedown, before its click.
const tried = ref(false)
function apply() {
  tried.value = true
  if (complete.value) emit('apply', { ...draft.value, key: key.value.trim() })
}
</script>

<template>
  <form class="flex flex-col gap-3" @submit.prevent="apply">
    <div class="flex flex-col gap-1">
      <span :class="caption" aria-hidden="true">{{ what }}</span>
      <Combobox
        v-model="key"
        :options="keyOptions"
        :label="what"
        :placeholder="kind === 'people' ? 'Pick a trait' : 'Pick a property'"
        allow-custom
        size="sm"
        name="filter-key"
      />
    </div>
    <div class="flex flex-col gap-1">
      <span :class="caption" aria-hidden="true">Comparison</span>
      <Select v-model="op" :options="ops" label="Comparison" size="sm" name="filter-op" />
    </div>
    <div v-if="op !== 'is_set'" class="flex flex-col gap-1">
      <span :class="caption" aria-hidden="true">Value</span>
      <Combobox
        v-model="value"
        :options="valueOptions"
        label="Value"
        :placeholder="numeric ? 'A number' : 'Pick or type a value'"
        allow-custom
        size="sm"
        name="filter-value"
      />
    </div>
    <p v-if="numeric" class="text-xs text-base-content/70">
      Greater and less than compare numbers; text values never match.
    </p>
    <p v-if="tried && !complete" role="alert" class="text-xs text-error">
      {{
        key.trim() === ''
          ? `Pick a ${what.toLowerCase()}.`
          : numeric
            ? 'Enter a number.'
            : 'Pick or type a value.'
      }}
    </p>
    <div class="flex justify-end gap-2">
      <button type="button" class="btn btn-ghost btn-sm" @click="emit('cancel')">Cancel</button>
      <button type="submit" class="btn btn-primary btn-sm">
        {{ applyLabel }}
      </button>
    </div>
  </form>
</template>
