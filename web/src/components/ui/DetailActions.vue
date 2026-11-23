<script setup lang="ts">
import { Check } from 'lucide-vue-next'
import type { Component } from 'vue'

import ActionsMenu from './ActionsMenu.vue'

/** A header button of a detail page. */
export interface DetailAction {
  label: string
  icon: Component
  run: () => void
  disabled?: boolean
}

/** The Save of a page that edits its entity inline. */
export interface DetailSave {
  /** The id of the form Save submits. */
  form: string
  dirty: boolean
  pending: boolean
  /** Why Save is off although the page has changes. */
  blocked?: string
}

/**
 * The action group in a detail page's header: page buttons, then Save, then
 * the "..." menu, all one size and style family. Save carries the save
 * state: "Save" with changes, a disabled "Saved" without, a spinner while
 * saving. Put `MenuItem`s in the `menu` slot, Delete last.
 */
const props = withDefaults(defineProps<{ actions?: DetailAction[]; save?: DetailSave }>(), {
  actions: () => [],
  save: undefined,
})
</script>

<template>
  <div class="flex flex-wrap items-center gap-2">
    <button
      v-for="action in props.actions"
      :key="action.label"
      type="button"
      class="btn gap-1.5 btn-sm"
      :disabled="action.disabled"
      @click="action.run"
    >
      <component :is="action.icon" class="size-4" aria-hidden="true" />{{ action.label }}
    </button>
    <button
      v-if="save"
      type="submit"
      :form="save.form"
      class="btn gap-1.5 btn-primary btn-sm"
      :disabled="!save.dirty || save.pending || save.blocked !== undefined"
      :aria-busy="save.pending"
      :title="save.dirty ? save.blocked : undefined"
    >
      <span
        v-if="save.pending"
        class="loading loading-xs loading-spinner"
        aria-hidden="true"
      ></span>
      <Check v-else-if="!save.dirty" class="size-4" aria-hidden="true" />
      {{ save.pending ? 'Saving' : save.dirty ? 'Save' : 'Saved' }}
    </button>
    <ActionsMenu v-if="$slots.menu"><slot name="menu" /></ActionsMenu>
  </div>
</template>
