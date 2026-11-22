import type { Component } from 'vue'
import type { RouteRecordRaw } from 'vue-router'

/**
 * Where a sidebar entry sits. `project` entries are the selected project's
 * content and render under the project switcher; `workspace` entries are
 * workspace meta (people, keys, settings) pinned to the bottom.
 */
export type NavSection = 'project' | 'workspace'

/** A sidebar entry inside a workspace. */
export interface NavItem {
  /**
   * The route path, relative to `/w/:workspaceId/p/:projectId/` for project
   * entries and to `/w/:workspaceId/` for workspace entries.
   */
  path: string
  label: string
  icon: Component
  section: NavSection
  /** Only workspace admins see it. */
  adminOnly?: boolean
  /**
   * Lower comes first within its section. Core uses 0-99; feature modules
   * start at 100. Settings keeps the highest workspace order so it sits last.
   */
  order: number
}

/**
 * What a feature contributes to the web app. Each backend module with a UI
 * gets one, listed in `modules/index.ts`; the router and the sidebar read
 * them, so a module never edits the shell.
 */
export interface WebModule {
  name: string
  /** Routes under `/w/:workspaceId/`, with relative paths. */
  workspaceRoutes?: RouteRecordRaw[]
  /**
   * Routes under `/w/:workspaceId/p/:projectId/`, with relative paths. Pages
   * get `projectId` (and any other params) as props.
   */
  projectRoutes?: RouteRecordRaw[]
  nav: NavItem[]
}

/** Typed identity, so module files get checked against `WebModule`. */
export function defineWebModule(module: WebModule): WebModule {
  return module
}
