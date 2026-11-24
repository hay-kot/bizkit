import type { InjectionKey } from 'vue'
import type { RouteLocationRaw } from 'vue-router'

import type { NavItem } from '@/modules/types'

/** One part of a page header's breadcrumb row. The last part is never a link. */
export interface Crumb {
  label: string
  to?: RouteLocationRaw
}

/**
 * What the workspace shell tells every `PageHeader` under it, so pages get
 * their breadcrumbs without passing them: the first crumb (the current
 * project, or the workspace on workspace pages) and the URL of a nav item.
 */
export interface PageContext {
  root?: Crumb
  navTo: (item: NavItem) => string
}

export const PAGE_CONTEXT: InjectionKey<PageContext> = Symbol('page-context')

/**
 * The default breadcrumb: the shell's root, then the page's module (its nav
 * item), then the page title when it is not the module's own page. Parts
 * without a label (a name still loading) drop out.
 */
export function defaultCrumbs(
  context: PageContext | undefined,
  nav: NavItem | undefined,
  title: string,
): Crumb[] {
  if (!context) return []
  const crumbs: Crumb[] = []
  if (context.root?.label) crumbs.push(context.root)
  if (nav) crumbs.push({ label: nav.label, to: context.navTo(nav) })
  if (title && title !== nav?.label) crumbs.push({ label: title })
  return crumbs
}
