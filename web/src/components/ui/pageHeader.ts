import { computed, inject, type ComputedRef, type InjectionKey } from 'vue'
import { routeLocationKey, type RouteLocationRaw } from 'vue-router'

import type { NavItem } from '@/modules/types'

/**
 * One part of a page header's breadcrumb row. The last part is never a
 * link; an empty label is a name still loading and shows as a skeleton.
 */
export interface Crumb {
  label: string
  to?: RouteLocationRaw
}

/**
 * What the workspace shell tells every `PageHeader` under it, so pages get
 * their breadcrumbs without passing them. Both take the page's own project
 * ID (none on workspace pages), so a page that is leaving keeps its crumbs
 * while the next route is already current.
 */
export interface PageContext {
  /** The first crumb: the project, or the workspace on workspace pages. */
  root: (projectId: string | undefined) => Crumb
  /** The URL of a nav item's page. */
  navTo: (item: NavItem, projectId: string | undefined) => string
}

export const PAGE_CONTEXT: InjectionKey<PageContext> = Symbol('page-context')

/** The parts of the route a header reads, taken once when the page mounts. */
export interface PageRoute {
  path: string
  projectId: string | undefined
  nav: NavItem | undefined
}

const trim = (path: string) => path.replace(/\/$/, '')

/**
 * The default breadcrumb: the shell's root, then the page's module (its nav
 * item), then the page title unless the page is the module's own page.
 * `withTitle: false` leaves the title out, for a tabbed layout whose tabs
 * already name the page.
 */
export function defaultCrumbs(
  context: PageContext,
  page: PageRoute,
  title: string,
  withTitle = true,
): Crumb[] {
  const crumbs: Crumb[] = [context.root(page.projectId)]
  const { nav } = page
  const navPath = nav ? context.navTo(nav, page.projectId) : undefined
  if (nav && navPath) crumbs.push({ label: nav.label, to: navPath })
  if (withTitle && (!navPath || trim(page.path) !== trim(navPath))) crumbs.push({ label: title })
  return crumbs
}

/**
 * The route this header belongs to. It is read once: during a route
 * transition the leaving page stays mounted while the router already shows
 * the next route, and its header must not switch to the next page's icon
 * and crumbs. Pages remount on every route change (`routeKey`), and a
 * tabbed layout reads only the module, which its tabs share.
 */
export function usePageRoute(): PageRoute | undefined {
  const route = inject(routeLocationKey, undefined)
  if (!route) return undefined
  const projectId = route.params.projectId
  return {
    path: route.path,
    projectId: typeof projectId === 'string' ? projectId : undefined,
    nav: route.meta.nav,
  }
}

/** The default crumbs for this page, or none outside the workspace shell. */
export function usePageCrumbs(title: () => string, withTitle = true): ComputedRef<Crumb[]> {
  const context = inject(PAGE_CONTEXT, undefined)
  const page = usePageRoute()
  return computed(() => (context && page ? defaultCrumbs(context, page, title(), withTitle) : []))
}
