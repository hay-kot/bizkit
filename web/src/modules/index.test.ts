import { describe, expect, it } from 'vitest'

import { modules, workspaceNav } from './index'

describe('web modules', () => {
  it('only link the sidebar to routes they register', () => {
    for (const module of modules) {
      const paths = module.workspaceRoutes.map((route) => route.path)
      for (const item of module.nav) {
        expect(paths, `${module.name} nav item ${item.label}`).toContain(item.path)
      }
    }
  })

  it('never collide on paths, names or sidebar order', () => {
    const paths = modules.flatMap((m) => m.workspaceRoutes.map((r) => r.path))
    expect(new Set(paths).size, 'route paths').toBe(paths.length)
    const names = modules.map((m) => m.name)
    expect(new Set(names).size, 'module names').toBe(names.length)
    const orders = workspaceNav.map((item) => item.order)
    expect(new Set(orders).size, 'nav order').toBe(orders.length)
  })

  it('use relative workspace paths', () => {
    for (const route of modules.flatMap((m) => m.workspaceRoutes)) {
      expect(route.path.startsWith('/'), route.path).toBe(false)
    }
  })
})
