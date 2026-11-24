import type { RouteLocationNormalized } from 'vue-router'

/** Matches the API's cap on tiles per dashboard. */
export const MAX_TILES = 20

interface SavedTile {
  insight_id: string
  width: number
}

/**
 * Saved tiles whose insight the insights list doesn't have yet. The editor
 * can't show them, so a save keeps them at the end and they count against
 * the cap.
 */
export function hiddenTiles<T extends SavedTile>(
  saved: readonly T[],
  known: ReadonlySet<string>,
): T[] {
  return saved.filter((tile) => !known.has(tile.insight_id))
}

/** How many more insights fit on the dashboard. */
export function tileRoom(draft: number, hidden: number): number {
  return Math.max(0, MAX_TILES - draft - hidden)
}

/**
 * Whether a route update opens another dashboard. Query changes (the range)
 * keep the page and its edits, so they never ask to discard.
 */
export function switchesDashboard(
  to: Pick<RouteLocationNormalized, 'params'>,
  from: Pick<RouteLocationNormalized, 'params'>,
): boolean {
  return to.params.dashboardId !== from.params.dashboardId
}
