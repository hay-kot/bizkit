import { ref } from 'vue'

import type { PaletteScope } from './types'

/** The scopes in tab order, with the sigil that enters each from an empty query. */
export const SCOPES = [
  {
    id: 'all',
    label: 'All',
    sigil: undefined,
    placeholder: 'Search pages, forms, scripts and actions',
  },
  { id: 'goto', label: 'Go to', sigil: '@', placeholder: 'Go to a page or an object' },
  { id: 'actions', label: 'Actions', sigil: '>', placeholder: 'Run an action' },
] as const satisfies readonly {
  id: PaletteScope | 'all'
  label: string
  sigil: string | undefined
  placeholder: string
}[]

export type ScopeId = (typeof SCOPES)[number]['id']

export const paletteOpen = ref(false)
export const paletteQuery = ref('')
export const paletteScope = ref<ScopeId>('all')

/** Opens the palette with an empty query, in `scope`. */
export function openPalette(scope: ScopeId = 'all'): void {
  paletteQuery.value = ''
  paletteScope.value = scope
  paletteOpen.value = true
}

export function closePalette(): void {
  paletteOpen.value = false
}
