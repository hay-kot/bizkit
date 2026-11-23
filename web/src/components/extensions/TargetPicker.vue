<script setup lang="ts">
import { computed } from 'vue'

import Checkbox from '@/components/ui/Checkbox.vue'
import type { HookTargetKind } from '@/queries/extensions'
import { useTargets } from '@/queries/extensions'

/**
 * Picks the entities a script runs for. An empty selection means every
 * entity of the kind, shown as the "All forms" box.
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
</script>

<template>
  <fieldset class="flex flex-col gap-2">
    <legend class="mb-1.5 text-sm font-medium">Runs for</legend>
    <Checkbox v-model="forAll" :label="`All ${kind.plural}`" />
    <p v-if="isPending" class="text-sm text-base-content/70">Loading {{ kind.plural }}...</p>
    <Checkbox
      v-for="target in targets?.items ?? []"
      :key="target.id"
      v-model="model"
      :value="target.id"
      :label="target.label"
    />
  </fieldset>
</template>
