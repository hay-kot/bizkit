/** The "On dashboard" filter's two values that aren't dashboard IDs. */
export const ANY_DASHBOARD = 'all'
export const NO_DASHBOARD = 'none'

interface Listed {
  name: string
  dashboards: { id: string }[]
}

/**
 * The insights whose name contains `search` (any case) and that sit on the
 * `dashboard` filter's dashboard, on none, or anywhere.
 */
export function filterInsights<T extends Listed>(
  items: T[],
  search: string,
  dashboard: string,
): T[] {
  const needle = search.trim().toLowerCase()
  return items.filter((insight) => {
    if (needle && !insight.name.toLowerCase().includes(needle)) return false
    if (dashboard === ANY_DASHBOARD) return true
    if (dashboard === NO_DASHBOARD) return insight.dashboards.length === 0
    return insight.dashboards.some((d) => d.id === dashboard)
  })
}

/**
 * `selected`, or every dashboard when it names one that `dashboardIds` no
 * longer has (deleted elsewhere), so the filter never hides every row behind
 * a choice the Select can't show. While the list loads, the choice stands.
 */
export function knownDashboardFilter(selected: string, dashboardIds?: string[]): string {
  if (selected === ANY_DASHBOARD || selected === NO_DASHBOARD || !dashboardIds) return selected
  return dashboardIds.includes(selected) ? selected : ANY_DASHBOARD
}
