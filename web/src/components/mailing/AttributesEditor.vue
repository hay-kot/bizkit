<script setup lang="ts">
import { Plus, X } from 'lucide-vue-next'

import Select from '@/components/ui/Select.vue'

import { computed } from 'vue'

import { type AttributeKind, type AttributeRow, rowsProblem } from './attributes'

/**
 * Key, type and value rows for contact attributes or segment conditions.
 * Keys are lower-case letters, digits and `_`; the server checks them.
 */
const props = defineProps<{ label: string; addLabel?: string; name: string; form?: string }>()
const rows = defineModel<AttributeRow[]>({ required: true })
const problem = computed(() => rowsProblem(rows.value))

const kinds: { value: AttributeKind; label: string }[] = [
  { value: 'text', label: 'Text' },
  { value: 'number', label: 'Number' },
  { value: 'bool', label: 'Yes or no' },
]
const bools = [
  { value: 'true', label: 'true' },
  { value: 'false', label: 'false' },
]

function add() {
  rows.value = [...rows.value, { key: '', kind: 'text', value: '' }]
}

function remove(index: number) {
  rows.value = rows.value.filter((_, i) => i !== index)
}

function setKind(index: number, kind: AttributeKind) {
  rows.value = rows.value.map((row, i) =>
    i === index ? { ...row, kind, value: kind === 'bool' ? 'true' : row.value } : row,
  )
}
</script>

<template>
  <fieldset class="flex flex-col gap-2">
    <legend class="mb-1 text-sm font-medium">{{ props.label }}</legend>
    <div v-for="(row, index) in rows" :key="index" class="flex items-center gap-2">
      <input
        v-model="row.key"
        :name="`${props.name}-key-${index}`"
        :form="props.form"
        :aria-label="`Key ${index + 1}`"
        placeholder="plan"
        pattern="[a-z][a-z0-9_]{0,39}"
        class="input w-32 font-mono input-sm"
      />
      <div class="w-32 shrink-0">
        <Select
          :model-value="row.kind"
          :options="kinds"
          :label="`Type ${index + 1}`"
          size="sm"
          @update:model-value="(kind) => setKind(index, kind as AttributeKind)"
        />
      </div>
      <div class="min-w-0 flex-1">
        <Select
          v-if="row.kind === 'bool'"
          v-model="row.value"
          :options="bools"
          :label="`Value ${index + 1}`"
          size="sm"
        />
        <input
          v-else
          v-model="row.value"
          :name="`${props.name}-value-${index}`"
          :form="props.form"
          :aria-label="`Value ${index + 1}`"
          :type="row.kind === 'number' ? 'number' : 'text'"
          step="any"
          class="input w-full input-sm"
        />
      </div>
      <button
        type="button"
        class="btn btn-square btn-ghost btn-sm"
        :aria-label="`Remove ${row.key || `row ${index + 1}`}`"
        @click="remove(index)"
      >
        <X class="size-4" aria-hidden="true" />
      </button>
    </div>
    <p v-if="problem" role="alert" class="text-sm text-error">{{ problem }}</p>
    <div>
      <button type="button" class="btn gap-1.5 btn-ghost btn-sm" @click="add">
        <Plus class="size-4" aria-hidden="true" />{{ props.addLabel ?? 'Add attribute' }}
      </button>
    </div>
  </fieldset>
</template>
