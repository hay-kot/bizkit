import type { RouteRecordRaw } from 'vue-router'

import analytics from './analytics'
import core from './core'
import extensions from './extensions'
import forms from './forms'
import mailing from './mailing'
import type { NavItem, NavSection, WebModule } from './types'
import webhooks from './webhooks'

/**
 * Tags a module's nav items and entity sections with its name, so the shell
 * can hide what a workspace turned off.
 */
function tagged(module: WebModule): WebModule {
  return {
    ...module,
    nav: module.nav.map((item) => ({ ...item, module: module.name })),
    entitySections: module.entitySections?.map((section) => ({
      ...section,
      module: module.name,
    })),
  }
}

/** Every web module, in the order their routes register. */
export const modules: WebModule[] = [core, forms, analytics, webhooks, mailing, extensions].map(
  tagged,
)

/**
 * Tags a module's route with the module's nav item it sits under (`forms`
 * and `forms/:formId` both under Forms). Child routes inherit the meta.
 */
function withNav(module: WebModule, route: RouteRecordRaw): RouteRecordRaw {
  const nav = module.nav.find(
    (item) => route.path === item.path || route.path.startsWith(`${item.path}/`),
  )
  return nav ? ({ ...route, meta: { ...route.meta, nav } } as RouteRecordRaw) : route
}

/** Every module's workspace routes, relative to `/w/:workspaceId/`. */
export const workspaceRoutes: RouteRecordRaw[] = modules.flatMap((m) =>
  (m.workspaceRoutes ?? []).map((route) => withNav(m, route)),
)

/** Params as props, on a route and its children. */
function withProps(route: RouteRecordRaw): RouteRecordRaw {
  return {
    ...route,
    props: route.props ?? true,
    ...(route.children ? { children: route.children.map(withProps) } : {}),
  } as RouteRecordRaw
}

/**
 * Every module's project routes, mounted under `p/:projectId/` with params
 * as props. A route with children (a tabbed layout) passes params to them too.
 */
export const projectRoutes: RouteRecordRaw[] = modules
  .flatMap((m) => (m.projectRoutes ?? []).map((route) => withNav(m, route)))
  .map((route) => withProps({ ...route, path: `p/:projectId/${route.path}` } as RouteRecordRaw))

function navFor(section: NavSection): NavItem[] {
  return modules
    .flatMap((m) => m.nav)
    .filter((item) => item.section === section)
    .sort((a, b) => a.order - b.order)
}

/** The selected project's sidebar entries, sorted by `order`. */
export const projectNav: NavItem[] = navFor('project')

/** The workspace meta entries pinned to the bottom of the sidebar, sorted by `order`. */
export const workspaceNav: NavItem[] = navFor('workspace')

/**
 * The URL of a project's page, `section` being a project nav path such as
 * `forms`. Without one, the first project page anyone can see, or the
 * Projects page when no module has project pages.
 */
export function projectPath(workspaceId: string, projectId: string, section?: string): string {
  const target = section ?? projectNav.find((item) => !item.adminOnly)?.path
  return target ? `/w/${workspaceId}/p/${projectId}/${target}` : `/w/${workspaceId}/projects`
}
