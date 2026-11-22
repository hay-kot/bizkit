import core from './core'
import type { NavItem, WebModule } from './types'

/** Every web module, in the order their routes register. */
export const modules: WebModule[] = [core]

/** Every module's workspace routes. */
export const workspaceRoutes = modules.flatMap((module) => module.workspaceRoutes)

/** The sidebar, sorted by `order`. */
export const workspaceNav: NavItem[] = modules
  .flatMap((module) => module.nav)
  .sort((a, b) => a.order - b.order)
