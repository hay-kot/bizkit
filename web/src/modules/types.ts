import type { Component } from 'vue'
import type { RouteRecordRaw } from 'vue-router'

/** A sidebar entry inside a workspace. */
export interface NavItem {
  /** The route path under `/w/:workspaceId/`, such as `projects`. */
  path: string
  label: string
  icon: Component
  /** Only workspace admins see it. */
  adminOnly?: boolean
  /** Lower comes first. Core uses 0-99; feature modules start at 100. */
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
  workspaceRoutes: RouteRecordRaw[]
  nav: NavItem[]
}

/** Typed identity, so module files get checked against `WebModule`. */
export function defineWebModule(module: WebModule): WebModule {
  return module
}
