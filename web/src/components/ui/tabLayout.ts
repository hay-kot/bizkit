import type { InjectionKey, Ref } from 'vue'

/** What a tab's page shows in the layout's header. */
export interface TabHeader {
  title: string
  description?: string
  width: 'wide' | 'narrow'
}

/** What `TabLayout` gives the `TabPage` of its current tab. */
export interface TabLayoutContext {
  header: TabHeader
  /** The header's actions row, where the page teleports its actions. */
  actions: Ref<HTMLElement | null>
}

export const TAB_LAYOUT: InjectionKey<TabLayoutContext> = Symbol('tab-layout')
