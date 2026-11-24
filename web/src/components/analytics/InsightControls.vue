<script setup lang="ts">
import { Plus, X } from 'lucide-vue-next'
import { computed } from 'vue'

import Select from '@/components/ui/Select.vue'
import type { BreakdownSource, FilterOp, InsightKind, Measure } from '@/queries/analytics'

import { emptyFilter, FILTER_OPS, type InsightDraft, MEASURES } from './insightDraft'

/** The insight builder's controls. `eventNames` fills the event picker. */
const props = defineProps<{ eventNames: string[] }>()
const draft = defineModel<InsightDraft>({ required: true })

const KINDS: { value: InsightKind; label: string; description: string }[] = [
  { value: 'events', label: 'Events', description: 'What people did in the range' },
  { value: 'people', label: 'People', description: 'Who people are now, by trait' },
]

const eventOptions = computed(() => {
  const names = new Set(props.eventNames)
  if (draft.value.event) names.add(draft.value.event)
  return [
    { value: '', label: 'Pick an event' },
    { value: 'pageview', label: 'pageview' },
    ...[...names].filter((n) => n !== 'pageview').map((name) => ({ value: name, label: name })),
  ]
})

const breakdownOptions = computed((): { value: BreakdownSource | ''; label: string }[] =>
  draft.value.kind === 'people'
    ? [
        { value: '', label: 'No breakdown' },
        { value: 'trait', label: 'By trait' },
      ]
    : [
        { value: '', label: 'No breakdown' },
        { value: 'property', label: 'By event property' },
        { value: 'trait', label: 'By person trait' },
      ],
)
const canBreakDown = computed(
  () => draft.value.kind === 'people' || draft.value.measure !== 'per_user',
)

function update(patch: Partial<InsightDraft>) {
  draft.value = { ...draft.value, ...patch }
}

const kind = computed({
  get: () => draft.value.kind,
  set: (value: InsightKind) =>
    update({
      kind: value,
      breakdownSource:
        draft.value.breakdownSource === 'property' ? '' : draft.value.breakdownSource,
    }),
})
const event = computed({
  get: () => draft.value.event,
  set: (value: string) => update({ event: value }),
})
const measure = computed({
  get: () => draft.value.measure,
  set: (value: Measure) => update({ measure: value }),
})
const breakdownSource = computed({
  get: () => draft.value.breakdownSource,
  set: (value: BreakdownSource | '') => update({ breakdownSource: value }),
})

function setFilter(at: number, patch: Partial<{ key: string; op: FilterOp; value: string }>) {
  update({ filters: draft.value.filters.map((f, i) => (i === at ? { ...f, ...patch } : f)) })
}
function addFilter() {
  update({ filters: [...draft.value.filters, emptyFilter()] })
}
function removeFilter(at: number) {
  update({ filters: draft.value.filters.filter((_, i) => i !== at) })
}
const filterWhat = computed(() => (draft.value.kind === 'people' ? 'Trait' : 'Property'))
</script>

<template>
  <div class="flex flex-col gap-3">
    <div class="flex flex-wrap items-center gap-2">
      <Select v-model="kind" :options="KINDS" label="Kind" size="sm" name="insight-kind" />
      <template v-if="draft.kind === 'events'">
        <Select
          v-model="event"
          :options="eventOptions"
          label="Event"
          size="sm"
          name="insight-event"
        />
        <Select
          v-model="measure"
          :options="MEASURES"
          label="Measure"
          size="sm"
          name="insight-measure"
        />
      </template>
      <template v-if="canBreakDown">
        <Select
          v-model="breakdownSource"
          :options="breakdownOptions"
          label="Breakdown"
          size="sm"
          name="insight-breakdown"
        />
        <input
          v-if="draft.breakdownSource"
          :value="draft.breakdownKey"
          name="insight-breakdown-key"
          class="input w-40 input-sm"
          :placeholder="draft.breakdownSource === 'trait' ? 'plan' : 'property'"
          :aria-label="
            draft.breakdownSource === 'trait'
              ? 'Trait to break down by'
              : 'Property to break down by'
          "
          @input="update({ breakdownKey: ($event.target as HTMLInputElement).value })"
        />
      </template>
    </div>

    <ul v-if="draft.filters.length" class="flex flex-col gap-2" aria-label="Filters">
      <li v-for="(filter, at) in draft.filters" :key="at" class="flex flex-wrap items-center gap-2">
        <input
          :value="filter.key"
          class="input w-40 input-sm"
          :placeholder="draft.kind === 'people' ? 'api_tokens' : 'plan'"
          :aria-label="`${filterWhat} name, filter ${at + 1}`"
          @input="setFilter(at, { key: ($event.target as HTMLInputElement).value })"
        />
        <Select
          :model-value="filter.op"
          :options="FILTER_OPS"
          :label="`Comparison, filter ${at + 1}`"
          size="sm"
          @update:model-value="(op: FilterOp) => setFilter(at, { op })"
        />
        <input
          v-if="filter.op !== 'is_set'"
          :value="filter.value"
          class="input w-40 input-sm"
          :placeholder="filter.op === 'gt' || filter.op === 'lt' ? '0' : 'pro'"
          :aria-label="`Value, filter ${at + 1}`"
          @input="setFilter(at, { value: ($event.target as HTMLInputElement).value })"
        />
        <button
          type="button"
          class="btn btn-square btn-ghost btn-sm"
          :aria-label="`Remove filter ${at + 1}`"
          @click="removeFilter(at)"
        >
          <X class="size-4" aria-hidden="true" />
        </button>
      </li>
    </ul>
    <div>
      <button type="button" class="btn gap-1.5 btn-ghost btn-sm" @click="addFilter">
        <Plus class="size-4" aria-hidden="true" />Add {{ filterWhat.toLowerCase() }} filter
      </button>
    </div>
    <p class="text-sm text-base-content/70">
      <template v-if="draft.kind === 'people'">
        People counts everyone known now, whatever the range. Send traits with identify.
      </template>
      <template v-else-if="draft.measure === 'share'">
        Active users are people with a stored ID (identified, or the snippet's anonymous ID) and any
        event in the range; cookieless visitors are left out.
      </template>
      <template v-else-if="draft.measure === 'per_user'">
        Counts people with a stored ID over raw events, which cover the last 90 days by default.
        Keep lifetime counts in traits.
      </template>
    </p>
  </div>
</template>
