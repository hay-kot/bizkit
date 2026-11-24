import type { InjectionKey, Ref } from 'vue'

/** What a tab's page shows in the layout's header. */
export interface TabHeader {
  title: string
  description?: string
}

/** What `TabLayout` gives the `TabPage` of its current tab. */
export interface TabLayoutContext {
  header: TabHeader
  /** The header's actions row, where the page teleports its actions. */
  actions: Ref<HTMLElement | null>
  /** How many mounted pages have actions; the row renders only then. */
  withActions: Ref<number>
  /** True while the old tab fades out, so its actions can't be clicked. */
  leaving: Ref<boolean>
}

export const TAB_LAYOUT: InjectionKey<TabLayoutContext> = Symbol('tab-layout')
