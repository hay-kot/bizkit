<script setup lang="ts">
import { Plus, Trash2 } from 'lucide-vue-next'

import Checkbox from '@/components/ui/Checkbox.vue'
import Select from '@/components/ui/Select.vue'
import type { SelectOption } from '@/components/ui/Select.vue'
import type { FieldDef, FieldKind } from '@/queries/forms'

/** Edits a form's field list: one row per field. */
const fields = defineModel<FieldDef[]>({ required: true })

const kinds: SelectOption<FieldKind>[] = [
  { value: 'text', label: 'Text' },
  { value: 'email', label: 'Email' },
  { value: 'url', label: 'URL' },
  { value: 'number', label: 'Number' },
  { value: 'choice', label: 'Choice' },
]

function add() {
  fields.value = [...fields.value, { name: '', label: '', kind: 'text', required: false }]
}

function remove(index: number) {
  fields.value = fields.value.filter((_, i) => i !== index)
}

function update(index: number, patch: Partial<FieldDef>) {
  fields.value = fields.value.map((field, i) => (i === index ? { ...field, ...patch } : field))
}

/** Options only belong to choice fields, and the server refuses them elsewhere. */
function setKind(index: number, kind: FieldKind) {
  update(index, kind === 'choice' ? { kind } : { kind, options: [], multiple: false })
}

function optionsText(field: FieldDef): string {
  return (field.options ?? []).join(', ')
}

function setOptions(index: number, text: string) {
  const options = text
    .split(',')
    .map((option) => option.trim())
    .filter((option) => option.length > 0)
  update(index, { options })
}
</script>

<template>
  <div class="flex flex-col gap-3">
    <p v-if="fields.length === 0" class="text-sm text-base-content/80">
      No fields defined: every field the site sends is kept.
    </p>
    <div
      v-for="(field, index) in fields"
      :key="index"
      class="flex flex-col gap-2 rounded-box border border-base-300 p-3"
      data-testid="field-row"
    >
      <div class="flex items-center gap-2">
        <input
          :value="field.name"
          class="input flex-1 input-sm"
          placeholder="name"
          :aria-label="`Field ${index + 1} name`"
          required
          @input="update(index, { name: ($event.target as HTMLInputElement).value })"
        />
        <Select
          :model-value="field.kind"
          :options="kinds"
          :label="`Field ${index + 1} kind`"
          size="sm"
          @update:model-value="setKind(index, $event)"
        />
        <button
          type="button"
          class="btn btn-square btn-ghost btn-sm"
          :aria-label="`Remove field ${index + 1}`"
          @click="remove(index)"
        >
          <Trash2 class="size-4" aria-hidden="true" />
        </button>
      </div>
      <input
        :value="field.label ?? ''"
        class="input w-full input-sm"
        placeholder="Label (optional)"
        :aria-label="`Field ${index + 1} label`"
        @input="update(index, { label: ($event.target as HTMLInputElement).value || null })"
      />
      <template v-if="field.kind === 'choice'">
        <input
          :value="optionsText(field)"
          class="input w-full input-sm"
          placeholder="Options, separated by commas"
          :aria-label="`Field ${index + 1} options`"
          @change="setOptions(index, ($event.target as HTMLInputElement).value)"
        />
        <Checkbox
          :model-value="field.multiple ?? false"
          label="Allow several"
          @update:model-value="update(index, { multiple: $event === true })"
        />
      </template>
      <Checkbox
        :model-value="field.required ?? false"
        label="Required"
        @update:model-value="update(index, { required: $event === true })"
      />
    </div>
    <div>
      <button type="button" class="btn gap-1.5 btn-ghost btn-sm" @click="add">
        <Plus class="size-4" aria-hidden="true" />Add field
      </button>
    </div>
  </div>
</template>
