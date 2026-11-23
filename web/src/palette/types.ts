import type { Component, MaybeRefOrGetter, Ref } from 'vue'

/** A filter over the one list of rows. `all` shows every row. */
export type PaletteScope = 'goto' | 'actions'

/**
 * One palette row. The title names the object or the action ("Contact",
 * "New form"); the group names its container ("Forms", "Website"); the
 * palette draws the path, so titles never repeat the group. Verbs and
 * synonyms go in `keywords`.
 */
export interface PaletteRow {
  /** Stable across reloads: recents remember rows by it. */
  id: string
  title: string
  group?: string
  /** Where the group sits, lower first. A group takes its rows' lowest. */
  order?: number
  keywords?: string[]
  icon?: Component
  /** A muted word for what the row is, such as `form` or `page`. */
  kind?: string
  /** Defaults to `actions`. */
  scope?: PaletteScope
  /** A route to go to. A row has `to` or `run`. */
  to?: string
  run?: () => void | Promise<void>
}

/**
 * What a module's palette rows can depend on. The host mounts the sources
 * again when the workspace or the project changes, so the IDs are fixed
 * for one call.
 */
export interface PaletteContext {
  workspaceId: string
  /** The selected project; undefined when the workspace has none. */
  projectId: string | undefined
  projectName: Ref<string>
  isAdmin: boolean
}

/**
 * A module's rows. Called in a component's setup, so it can use the query
 * composables, and only when its result may change with the context.
 */
export type PaletteSource = (cx: PaletteContext) => MaybeRefOrGetter<PaletteRow[]>
