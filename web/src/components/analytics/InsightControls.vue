<script setup lang="ts">
import { Plus, X } from 'lucide-vue-next'
import { computed } from 'vue'

import Combobox, { type ComboOption } from '@/components/ui/Combobox.vue'
import Popover from '@/components/ui/Popover.vue'
import SegmentedControl from '@/components/ui/SegmentedControl.vue'
import Select from '@/components/ui/Select.vue'
import type { DayRange } from '@/lib/range'
import {
  type BreakdownSource,
  eventOptions,
  type InsightKind,
  type Measure,
  useSchemaEvents,
  useSchemaKeys,
} from '@/queries/analytics'

import InsightFilterForm from './InsightFilterForm.vue'
import {
  describeFilter,
  emptyFilter,
  type FilterDraft,
  type InsightDraft,
  MAX_FILTERS,
  MEASURES,
} from './insightDraft'

/**
 * The guided builder: one labelled row per part of the query, every name
 * picked from the project's schema over the range.
 */
const props = defineProps<{ workspaceId: string; projectId: string; range: DayRange }>()
const draft = defineModel<InsightDraft>({ required: true })

const KINDS: { value: InsightKind; label: string }[] = [
  { value: 'events', label: 'Events' },
  { value: 'people', label: 'People' },
]

function update(patch: Partial<InsightDraft>) {
  draft.value = { ...draft.value, ...patch }
}

const kind = computed({
  get: () => draft.value.kind,
  set: (value: InsightKind) =>
    update({
      kind: value,
      ...(draft.value.breakdownSource === 'property'
        ? { breakdownSource: '' as const, breakdownKey: '' }
        : {}),
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

const span = computed(() => ({ from: props.range.from, to: props.range.to }))
const events = useSchemaEvents(
  () => props.workspaceId,
  () => props.projectId,
  () => ({ ...span.value, limit: 200 }),
)
const eventChoices = computed((): ComboOption[] => {
  const options: ComboOption[] = eventOptions(events.data.value?.items ?? [])
  const current = draft.value.event
  // A linked or saved event that the range never saw still shows as picked.
  if (current && !options.some((o) => o.value === current)) {
    options.unshift({
      value: current,
      group: current === 'pageview' ? 'Page views' : 'Custom events',
    })
  }
  return options
})

const propertyKeys = useSchemaKeys(
  () => props.workspaceId,
  () => props.projectId,
  () => ({ source: 'property', event: draft.value.event || undefined, limit: 100, ...span.value }),
)
const traitKeys = useSchemaKeys(
  () => props.workspaceId,
  () => props.projectId,
  { source: 'trait', limit: 100 },
)

const NONE = ''
const SEP = ':'
function breakdownValue(source: BreakdownSource | '', key: string): string {
  return source && key ? `${source}${SEP}${key}` : NONE
}
const breakdownChoices = computed((): ComboOption[] => {
  const options: ComboOption[] = [{ value: NONE, label: 'None' }]
  if (draft.value.kind === 'events') {
    for (const k of propertyKeys.data.value?.items ?? []) {
      options.push({
        value: breakdownValue('property', k.key),
        label: k.key,
        count: k.count,
        group: 'Event properties',
      })
    }
  }
  for (const k of traitKeys.data.value?.items ?? []) {
    options.push({
      value: breakdownValue('trait', k.key),
      label: k.key,
      count: k.count,
      group: 'Person traits',
    })
  }
  const current = breakdown.value
  if (current !== NONE && !options.some((o) => o.value === current)) {
    options.push({
      value: current,
      label: draft.value.breakdownKey,
      group: draft.value.breakdownSource === 'trait' ? 'Person traits' : 'Event properties',
    })
  }
  return options
})
const breakdown = computed({
  get: () => breakdownValue(draft.value.breakdownSource, draft.value.breakdownKey),
  set: (value: string) => {
    const at = value.indexOf(SEP)
    if (value === NONE || at < 0) {
      update({ breakdownSource: '', breakdownKey: '' })
      return
    }
    update({
      breakdownSource: value.slice(0, at) as BreakdownSource,
      breakdownKey: value.slice(at + 1),
    })
  },
})
const canBreakDown = computed(
  () => draft.value.kind === 'people' || draft.value.measure !== 'per_user',
)

function setFilter(at: number, filter: FilterDraft) {
  update({ filters: draft.value.filters.map((f, i) => (i === at ? filter : f)) })
}
function addFilter(filter: FilterDraft) {
  update({ filters: [...draft.value.filters, filter] })
}
function removeFilter(at: number) {
  update({ filters: draft.value.filters.filter((_, i) => i !== at) })
}
function applyAndClose(change: () => void, close: () => void) {
  change()
  close()
}
const full = computed(() => draft.value.filters.length >= MAX_FILTERS)

const label = 'w-28 shrink-0 pt-2 text-sm text-base-content/70'
const row = 'flex flex-col gap-1 sm:flex-row sm:gap-4'
const hint = 'text-sm text-base-content/70'
const chip =
  'flex h-8 items-center px-2.5 text-sm transition-colors hover:bg-base-content/10 focus-visible:outline-2 focus-visible:outline-primary'
</script>

<template>
  <section
    class="flex flex-col gap-4 rounded-box border border-base-300 bg-base-100 p-4"
    aria-label="Query builder"
  >
    <div :class="row">
      <span :class="label">Count</span>
      <div class="flex flex-col gap-1.5">
        <div><SegmentedControl v-model="kind" :options="KINDS" label="Count" /></div>
        <p v-if="draft.kind === 'people'" :class="hint">
          People counts everyone known now, whatever the range. Send traits with identify.
        </p>
      </div>
    </div>

    <template v-if="draft.kind === 'events'">
      <div :class="row">
        <span :class="label">Event</span>
        <Combobox
          v-model="event"
          :options="eventChoices"
          label="Event"
          name="insight-event"
          placeholder="Pick an event"
          size="toolbar"
          class="w-full sm:w-72"
        />
      </div>
      <div :class="row">
        <span :class="label">Measure</span>
        <div class="flex min-w-0 flex-col gap-1.5">
          <Select
            v-model="measure"
            :options="MEASURES"
            label="Measure"
            size="toolbar"
            name="insight-measure"
            class="w-full sm:w-72"
          />
          <p v-if="draft.measure === 'share'" :class="hint">
            Active users are people with a stored ID (identified, or the snippet's anonymous ID) and
            any event in the range; cookieless visitors are left out.
          </p>
          <p v-else-if="draft.measure === 'per_user'" :class="hint">
            Counts people with a stored ID over raw events, which cover the last 90 days by default.
            Keep lifetime counts in traits.
          </p>
        </div>
      </div>
    </template>

    <div :class="row">
      <span :class="label">Filters</span>
      <div class="flex min-w-0 flex-col gap-1.5">
        <ul class="flex flex-wrap items-center gap-2" aria-label="Filters">
          <li
            v-for="(filter, at) in draft.filters"
            :key="`${at}:${filter.key}`"
            class="flex items-center overflow-hidden rounded-field border border-base-300 bg-base-200"
          >
            <Popover :label="`Edit filter ${describeFilter(filter)}`">
              <template #trigger="{ toggle, controls, open }">
                <button
                  type="button"
                  :class="chip"
                  aria-haspopup="dialog"
                  :aria-expanded="open"
                  :aria-controls="controls"
                  :aria-label="`Edit filter ${describeFilter(filter)}`"
                  @click="toggle"
                >
                  {{ describeFilter(filter) }}
                </button>
              </template>
              <template #default="{ close }">
                <InsightFilterForm
                  :workspace-id="workspaceId"
                  :project-id="projectId"
                  :range="range"
                  :kind="draft.kind"
                  :event="draft.event"
                  :initial="filter"
                  apply-label="Apply"
                  @apply="(f: FilterDraft) => applyAndClose(() => setFilter(at, f), close)"
                  @cancel="close()"
                />
              </template>
            </Popover>
            <button
              type="button"
              class="flex size-8 items-center justify-center border-l border-base-300 text-base-content/70 transition-colors hover:bg-base-content/10 hover:text-base-content focus-visible:outline-2 focus-visible:outline-primary"
              :aria-label="`Remove filter ${describeFilter(filter)}`"
              @click="removeFilter(at)"
            >
              <X class="size-3.5" aria-hidden="true" />
            </button>
          </li>
          <li>
            <Popover label="Add filter">
              <template #trigger="{ toggle, controls, open }">
                <button
                  type="button"
                  class="btn h-8 gap-1.5 btn-ghost btn-sm"
                  aria-haspopup="dialog"
                  :aria-expanded="open"
                  :aria-controls="controls"
                  :disabled="full"
                  @click="toggle"
                >
                  <Plus class="size-4" aria-hidden="true" />Add filter
                </button>
              </template>
              <template #default="{ close }">
                <InsightFilterForm
                  :workspace-id="workspaceId"
                  :project-id="projectId"
                  :range="range"
                  :kind="draft.kind"
                  :event="draft.event"
                  :initial="emptyFilter()"
                  apply-label="Add filter"
                  @apply="(f: FilterDraft) => applyAndClose(() => addFilter(f), close)"
                  @cancel="close()"
                />
              </template>
            </Popover>
          </li>
        </ul>
        <p :class="hint">
          <template v-if="full">At most {{ MAX_FILTERS }} filters. </template>
          <template v-if="draft.kind === 'people'">Every filter is on a person trait.</template>
          <template v-else>Every filter is on a property of the event.</template>
        </p>
      </div>
    </div>

    <div :class="row">
      <span :class="label">Breakdown</span>
      <Combobox
        v-if="canBreakDown"
        v-model="breakdown"
        :options="breakdownChoices"
        label="Breakdown"
        name="insight-breakdown"
        placeholder="None"
        size="toolbar"
        class="w-full sm:w-72"
      />
      <p v-else :class="[hint, 'pt-2']">A per-user distribution has no breakdown.</p>
    </div>
  </section>
</template>
