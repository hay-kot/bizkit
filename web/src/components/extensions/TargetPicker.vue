<script setup lang="ts">
import { computed } from 'vue'

import Checkbox from '@/components/ui/Checkbox.vue'
import type { HookTargetKind } from '@/queries/extensions'
import { useTargets } from '@/queries/extensions'

/**
 * Picks the entities a script runs for. An empty selection means every
 * entity of the kind, shown as the "All forms" box. The last ticked entity
 * can't be unticked, because that would silently widen the script to all of
 * them; ticking "All forms" is the way there.
 */
const props = defineProps<{ workspaceId: string; projectId: string; kind: HookTargetKind }>()
const model = defineModel<string[]>({ required: true })
const { data: targets, isPending } = useTargets(
  () => props.workspaceId,
  () => props.projectId,
  () => props.kind.id,
)

const forAll = computed({
  get: () => model.value.length === 0,
  set: (all: boolean) => {
    if (all) model.value = []
  },
})

/** Live entities, then selected ones that no longer exist, so they can be unticked. */
const options = computed(() => {
  const live = (targets.value?.items ?? []).map((t) => ({ id: t.id, label: t.label }))
  const known = new Set(live.map((t) => t.id))
  const gone = model.value
    .filter((id) => !known.has(id))
    .map((id) => ({ id, label: `Deleted ${props.kind.noun}` }))
  return targets.value ? [...live, ...gone] : []
})

function isLast(id: string): boolean {
  return model.value.length === 1 && model.value[0] === id
}
</script>

<template>
  <fieldset class="flex flex-col gap-2">
    <legend class="mb-1.5 text-sm font-medium">Runs for</legend>
    <Checkbox v-model="forAll" :label="`All ${kind.plural}`" />
    <p v-if="isPending" class="text-sm text-base-content/70">Loading {{ kind.plural }}...</p>
    <Checkbox
      v-for="option in options"
      :key="option.id"
      v-model="model"
      :value="option.id"
      :label="option.label"
      :disabled="isLast(option.id)"
    />
    <p v-if="model.length === 1" class="text-xs text-base-content/70">
      To run for every {{ kind.noun }}, tick All {{ kind.plural }}.
    </p>
  </fieldset>
</template>
