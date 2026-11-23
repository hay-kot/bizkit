import { ref, watch } from 'vue'

const KEY = 'bizkit.palette.recents'
/** How many rows the palette remembers. */
export const RECENT_LIMIT = 8

function load(): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(KEY) ?? '[]')
    return Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : []
  } catch {
    return []
  }
}

/** The ids of the rows run last, newest first. */
export const recents = ref<string[]>(load())

watch(
  recents,
  (ids) => {
    try {
      localStorage.setItem(KEY, JSON.stringify(ids))
    } catch {
      // Storage can be full or blocked; recents then last for the session.
    }
  },
  { deep: true },
)

/** Puts `id` first, once, and keeps the newest {@link RECENT_LIMIT}. */
export function recordRun(id: string): void {
  recents.value = [id, ...recents.value.filter((seen) => seen !== id)].slice(0, RECENT_LIMIT)
}

/** Reads storage again; for tests. */
export function reloadRecents(): void {
  recents.value = load()
}
