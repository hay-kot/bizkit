import { inject, type InjectionKey } from 'vue'

import type { EntitySection } from './types'

/**
 * The entity sections of every module, provided by main.ts. Module pages
 * render `EntitySections`, and the registry imports those pages, so the
 * component gets the list through injection: importing `@/modules` there
 * makes an import cycle that breaks Vite's hot reload.
 */
export const entitySectionsKey: InjectionKey<EntitySection[]> = Symbol('entitySections')

export function useEntitySections(): EntitySection[] {
  return inject(entitySectionsKey, [])
}
