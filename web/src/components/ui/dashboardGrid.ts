import type { Segment } from './SegmentedControl.vue'

/** A tile's width in columns of the 12-column dashboard grid. */
export type TileWidth = 4 | 6 | 8 | 12

export const TILE_WIDTHS: Segment<TileWidth>[] = [
  { value: 4, label: '1/3', description: 'One third' },
  { value: 6, label: '1/2', description: 'Half' },
  { value: 8, label: '2/3', description: 'Two thirds' },
  { value: 12, label: 'Full', description: 'Full width' },
]

export function isTileWidth(width: number): width is TileWidth {
  return TILE_WIDTHS.some((option) => option.value === width)
}

/** A copy of `list` with the entry at `from` moved to index `to`. */
export function moveItem<T>(list: readonly T[], from: number, to: number): T[] {
  const next = [...list]
  const [item] = next.splice(from, 1)
  if (item === undefined) return next
  next.splice(Math.max(0, Math.min(to, next.length)), 0, item)
  return next
}

/** Where a dragged tile lands: beside the tile at `index`. */
export interface DropTarget {
  index: number
  after: boolean
}

/**
 * The index the tile dragged from `from` ends up at when dropped on
 * `target`, or `undefined` when the drop leaves it where it is.
 */
export function dropIndex(from: number, target: DropTarget): number | undefined {
  const insertAt = target.index + (target.after ? 1 : 0)
  const to = insertAt > from ? insertAt - 1 : insertAt
  return to === from ? undefined : to
}
