import {
  type MaybeRefOrGetter,
  computed,
  getCurrentScope,
  onScopeDispose,
  shallowRef,
  toValue,
} from 'vue'

import type { PaletteRow } from './types'

interface Registration {
  key: symbol
  source: MaybeRefOrGetter<PaletteRow[]>
}

const registrations = shallowRef<Registration[]>([])

/**
 * Adds rows to the palette for as long as the calling component or effect
 * scope lives. Pass a computed or a getter so rows follow their data.
 */
export function usePaletteRows(source: MaybeRefOrGetter<PaletteRow[]>): void {
  const key = Symbol('palette rows')
  registrations.value = [...registrations.value, { key, source }]
  if (getCurrentScope()) {
    onScopeDispose(() => {
      registrations.value = registrations.value.filter((r) => r.key !== key)
    })
  }
}

/** Every registered row, in registration order. Later rows with a taken id are dropped. */
export const paletteRows = computed((): PaletteRow[] => {
  const seen = new Set<string>()
  const rows: PaletteRow[] = []
  for (const registration of registrations.value) {
    for (const row of toValue(registration.source)) {
      if (seen.has(row.id)) continue
      seen.add(row.id)
      rows.push(row)
    }
  }
  return rows
})
