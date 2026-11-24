import { describe, expect, it } from 'vitest'
import type { RouteRecordRaw } from 'vue-router'

import { modules, projectNav, workspaceNav } from './index'

/** A module's routes with children joined to their parent's path. */
function flat(routes: RouteRecordRaw[] = []): RouteRecordRaw[] {
  return routes.flatMap((route) => [
    route,
    ...flat(route.children).map((child) => ({
      ...child,
      path: child.path ? `${route.path}/${child.path}` : route.path,
    })),
  ]) as RouteRecordRaw[]
}

/** Paths a router resolves: leaves, not the layouts that wrap them. */
const leaves = (routes: RouteRecordRaw[] = []) =>
  flat(routes).filter((route) => !route.children?.length)

describe('web modules', () => {
  it('only link the sidebar to routes they register in the same section', () => {
    for (const module of modules) {
      const routes = {
        project: (module.projectRoutes ?? []).map((route) => route.path),
        workspace: (module.workspaceRoutes ?? []).map((route) => route.path),
      }
      for (const item of module.nav) {
        expect(routes[item.section], `${module.name} nav item ${item.label}`).toContain(item.path)
      }
    }
  })

  it('never collide on paths, names or sidebar order', () => {
    for (const key of ['projectRoutes', 'workspaceRoutes'] as const) {
      const paths = modules.flatMap((m) => leaves(m[key]).map((r) => r.path))
      expect(new Set(paths).size, key).toBe(paths.length)
    }
    const names = modules.map((m) => m.name)
    expect(new Set(names).size, 'module names').toBe(names.length)
    const orders = [...projectNav, ...workspaceNav].map((item) => item.order)
    expect(new Set(orders).size, 'nav order').toBe(orders.length)
  })

  it('give entity sections unique keys', () => {
    const keys = modules.flatMap((m) => (m.entitySections ?? []).map((section) => section.key))
    expect(new Set(keys).size, 'entity section keys').toBe(keys.length)
  })

  it('keep the workspace prefix p/ free for project routes', () => {
    for (const route of modules.flatMap((m) => m.workspaceRoutes ?? [])) {
      expect(route.path === 'p' || route.path.startsWith('p/'), route.path).toBe(false)
    }
  })

  it('use relative paths', () => {
    const all = (routes: RouteRecordRaw[] = []): RouteRecordRaw[] =>
      routes.flatMap((route) => [route, ...all(route.children)])
    for (const route of modules.flatMap((m) => [
      ...all(m.workspaceRoutes),
      ...all(m.projectRoutes),
    ])) {
      expect(route.path.startsWith('/'), route.path).toBe(false)
    }
  })

  it('pin Settings to the bottom of the sidebar', () => {
    expect(workspaceNav.at(-1)?.path).toBe('settings')
  })

  it('stay out of the imports of the pages and components they register', () => {
    // The registry imports every module page, so a page that imports it back
    // makes a cycle: fine on a full load, but Vite's hot reload re-runs the
    // cycle and fails. The shell's own files are not module pages.
    const shell = [
      '/src/components/PaletteRows.vue',
      '/src/components/ProjectSwitcher.vue',
      '/src/pages/workspace/WorkspaceHome.vue',
    ]
    const sources = import.meta.glob<string>('/src/{pages,components}/**/*.vue', {
      query: '?raw',
      import: 'default',
      eager: true,
    })
    const importers = Object.entries(sources)
      .filter(([path, source]) => !shell.includes(path) && /from '@\/modules'/.test(source))
      .map(([path]) => path)
    expect(importers).toEqual([])
  })
})
