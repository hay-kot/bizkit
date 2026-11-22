import { describe, expect, it } from 'vitest'

import { modules, projectNav, workspaceNav } from './index'

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
      const paths = modules.flatMap((m) => (m[key] ?? []).map((r) => r.path))
      expect(new Set(paths).size, key).toBe(paths.length)
    }
    const names = modules.map((m) => m.name)
    expect(new Set(names).size, 'module names').toBe(names.length)
    const orders = [...projectNav, ...workspaceNav].map((item) => item.order)
    expect(new Set(orders).size, 'nav order').toBe(orders.length)
  })

  it('keep the workspace prefix p/ free for project routes', () => {
    for (const route of modules.flatMap((m) => m.workspaceRoutes ?? [])) {
      expect(route.path === 'p' || route.path.startsWith('p/'), route.path).toBe(false)
    }
  })

  it('use relative paths', () => {
    for (const route of modules.flatMap((m) => [
      ...(m.workspaceRoutes ?? []),
      ...(m.projectRoutes ?? []),
    ])) {
      expect(route.path.startsWith('/'), route.path).toBe(false)
    }
  })

  it('pin Settings to the bottom of the sidebar', () => {
    expect(workspaceNav.at(-1)?.path).toBe('settings')
  })
})
