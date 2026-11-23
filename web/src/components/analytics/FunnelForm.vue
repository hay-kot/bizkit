<script setup lang="ts">
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-vue-next'

import Field from '@/components/ui/Field.vue'
import Select from '@/components/ui/Select.vue'
import { type FunnelDraft, MAX_STEPS, MIN_STEPS } from './funnelDraft'

/** The fields of the funnel create and edit panels. */
const draft = defineModel<FunnelDraft>({ required: true })

const kinds = [
  { value: 'page' as const, label: 'Page' },
  { value: 'event' as const, label: 'Event' },
]

function move(index: number, by: number) {
  const steps = [...draft.value.steps]
  const [step] = steps.splice(index, 1)
  if (!step) return
  steps.splice(index + by, 0, step)
  draft.value = { ...draft.value, steps }
}

function remove(index: number) {
  draft.value = { ...draft.value, steps: draft.value.steps.filter((_, i) => i !== index) }
}

function add() {
  draft.value = { ...draft.value, steps: [...draft.value.steps, { kind: 'event', match: '' }] }
}
</script>

<template>
  <Field label="Name">
    <input
      v-model="draft.name"
      name="funnel-name"
      required
      maxlength="100"
      class="input w-full"
      placeholder="Signup"
    />
  </Field>
  <Field label="Window" hint="Days after the first step in which the rest must happen.">
    <input
      v-model.number="draft.windowDays"
      name="funnel-window"
      type="number"
      min="1"
      max="90"
      required
      class="input w-32"
    />
  </Field>
  <fieldset class="flex flex-col gap-2">
    <legend class="mb-2 text-sm font-medium">Steps</legend>
    <p class="text-sm text-base-content/70">
      A page path (exact, or a prefix ending in *) or an event name, in order.
    </p>
    <ol class="flex flex-col gap-2">
      <li v-for="(step, i) in draft.steps" :key="i" class="flex items-center gap-2">
        <span class="w-5 text-sm text-base-content/70 tabular-nums">{{ i + 1 }}</span>
        <Select v-model="step.kind" :options="kinds" :label="`Step ${i + 1} kind`" size="sm" />
        <input
          v-model="step.match"
          :name="`funnel-step-${i + 1}`"
          required
          class="input flex-1 input-sm"
          :placeholder="step.kind === 'page' ? '/pricing' : 'signup_completed'"
          :aria-label="`Step ${i + 1} ${step.kind === 'page' ? 'path' : 'event name'}`"
        />
        <button
          type="button"
          class="btn btn-square btn-ghost btn-xs"
          :disabled="i === 0"
          :aria-label="`Move step ${i + 1} up`"
          @click="move(i, -1)"
        >
          <ArrowUp class="size-4" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="btn btn-square btn-ghost btn-xs"
          :disabled="i === draft.steps.length - 1"
          :aria-label="`Move step ${i + 1} down`"
          @click="move(i, 1)"
        >
          <ArrowDown class="size-4" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="btn btn-square btn-ghost btn-xs"
          :disabled="draft.steps.length <= MIN_STEPS"
          :aria-label="`Remove step ${i + 1}`"
          @click="remove(i)"
        >
          <Trash2 class="size-4" aria-hidden="true" />
        </button>
      </li>
    </ol>
    <button
      type="button"
      class="btn self-start btn-ghost btn-sm"
      :disabled="draft.steps.length >= MAX_STEPS"
      @click="add"
    >
      <Plus class="size-4" aria-hidden="true" />Add step
    </button>
  </fieldset>
</template>
