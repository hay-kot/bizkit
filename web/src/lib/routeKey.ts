import type { RouteLocationNormalizedLoaded } from 'vue-router'

/**
 * The key for the component a `RouterView` at `depth` renders. A leaf page
 * keys by the whole path, so each page remounts. A parent route (a tabbed
 * layout) keys by its own path with params filled in, so it stays mounted
 * while its children change.
 */
export function routeKey(route: RouteLocationNormalizedLoaded, depth: number): string {
  const record = route.matched[depth]
  if (!record || record.children.length === 0) return route.path
  return record.path.replace(/:(\w+)(\([^)]*\))?[?*+]?/g, (_, name: string) => {
    const value = route.params[name]
    return Array.isArray(value) ? value.join('/') : (value ?? '')
  })
}
