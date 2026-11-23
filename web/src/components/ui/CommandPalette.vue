<script setup lang="ts">
import { useEventListener, useScrollLock } from '@vueuse/core'
import { CornerDownLeft, Search } from 'lucide-vue-next'
import { computed, nextTick, onBeforeUnmount, ref, useId, watch } from 'vue'
import { useRouter } from 'vue-router'

import { rank, titleMatches } from '@/palette/match'
import { recents, recordRun } from '@/palette/recents'
import { paletteRows } from '@/palette/registry'
import {
  SCOPES,
  closePalette,
  paletteOpen,
  paletteQuery,
  paletteScope,
  type ScopeId,
} from '@/palette/state'
import type { PaletteRow } from '@/palette/types'

/**
 * The command palette: one list of registered rows, filtered by a scope and
 * ranked by the query. Rows come from `usePaletteRows` and the modules'
 * `palette` sources; open it with `openPalette` or Cmd/Ctrl+K.
 */

interface Section {
  label: string
  rows: PaletteRow[]
  /** Draw `group ›` before each title: the section mixes groups. */
  paths: boolean
}

const router = useRouter()
const listId = useId()
const input = ref<HTMLInputElement>()
const selectedId = ref<string>()
const lock = useScrollLock(typeof document === 'undefined' ? null : document.body)
let restoreFocus: HTMLElement | null = null
let ranCommand = false
let lastPointer = { x: -1, y: -1 }

const scope = computed(() => SCOPES.find((s) => s.id === paletteScope.value) ?? SCOPES[0])

const inScope = computed(() =>
  paletteRows.value.filter(
    (row) => paletteScope.value === 'all' || (row.scope ?? 'actions') === paletteScope.value,
  ),
)

const sections = computed((): Section[] => {
  const query = paletteQuery.value.trim()
  if (query !== '') {
    const rows = rank(inScope.value, query)
    return rows.length > 0 ? [{ label: 'Results', rows, paths: true }] : []
  }
  const out: Section[] = []
  let rest = inScope.value
  if (paletteScope.value === 'all') {
    const byId = new Map(rest.map((row) => [row.id, row]))
    const recent = recents.value.map((id) => byId.get(id)).filter((row) => row !== undefined)
    if (recent.length > 0) {
      out.push({ label: 'Recent', rows: recent, paths: true })
      const taken = new Set(recent.map((row) => row.id))
      rest = rest.filter((row) => !taken.has(row.id))
    }
  }
  for (const row of rank(rest, '')) {
    const label = row.group ?? 'Other'
    const last = out[out.length - 1]
    if (last && last.label === label && !last.paths) last.rows.push(row)
    else out.push({ label, rows: [row], paths: false })
  }
  return out
})

const ordered = computed(() => sections.value.flatMap((section) => section.rows))

const optionId = (row: PaletteRow) => `${listId}-${row.id}`

watch(
  ordered,
  (rows) => {
    if (!rows.some((row) => row.id === selectedId.value)) selectedId.value = rows[0]?.id
  },
  { immediate: true },
)
watch([paletteQuery, paletteScope], () => (selectedId.value = ordered.value[0]?.id))

// An open panel under the palette holds its own lock on the body. Lock only
// when nothing else has, so closing the palette never unlocks the panel's.
let ownsLock = false

watch(paletteOpen, async (open) => {
  if (open) {
    ownsLock = !lock.value && document.body.style.overflow !== 'hidden'
    if (ownsLock) lock.value = true
  } else if (ownsLock) {
    lock.value = false
    ownsLock = false
  }
  if (open) {
    ranCommand = false
    restoreFocus = document.activeElement as HTMLElement | null
    selectedId.value = ordered.value[0]?.id
    await nextTick()
    input.value?.focus()
  } else if (!ranCommand && restoreFocus?.isConnected) {
    restoreFocus.focus()
  }
})

// Capture phase, so an open panel or menu under the palette doesn't also
// close on the same Escape: their listeners sit on window's bubble phase.
useEventListener(
  window,
  'keydown',
  (event: KeyboardEvent) => {
    if (event.key !== 'Escape' || !paletteOpen.value) return
    event.preventDefault()
    event.stopPropagation()
    closePalette()
  },
  { capture: true },
)

// The open flag is module state; leaving the workspace must not carry an
// open palette into the next layout, which would skip the open watcher.
onBeforeUnmount(closePalette)

function move(by: number) {
  const rows = ordered.value
  if (rows.length === 0) return
  const at = rows.findIndex((row) => row.id === selectedId.value)
  const next = rows[(at + by + rows.length) % rows.length]
  selectedId.value = next?.id
  void nextTick(() =>
    document.getElementById(next ? optionId(next) : '')?.scrollIntoView({ block: 'nearest' }),
  )
}

function cycleScope(by: number) {
  const at = SCOPES.findIndex((s) => s.id === paletteScope.value)
  paletteScope.value = SCOPES[(at + by + SCOPES.length) % SCOPES.length]!.id
}

async function runRow(row: PaletteRow) {
  ranCommand = true
  recordRun(row.id)
  closePalette()
  if (row.to !== undefined) await router.push(row.to)
  else await row.run?.()
}

function onKeydown(event: KeyboardEvent) {
  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault()
      move(1)
      break
    case 'ArrowUp':
      event.preventDefault()
      move(-1)
      break
    case 'Enter': {
      event.preventDefault()
      const row = ordered.value.find((r) => r.id === selectedId.value)
      if (row) void runRow(row)
      break
    }
    case 'Tab':
      event.preventDefault()
      cycleScope(event.shiftKey ? -1 : 1)
      break
    case 'Backspace':
      if (paletteQuery.value === '' && paletteScope.value !== 'all') {
        event.preventDefault()
        paletteScope.value = 'all'
      }
      break
  }
}

/** A sigil typed first into an empty query enters its scope. */
function onInput(event: Event) {
  const value = (event.target as HTMLInputElement).value
  const sigil = SCOPES.find((s) => s.sigil !== undefined && value.startsWith(s.sigil))
  if (paletteQuery.value === '' && paletteScope.value === 'all' && sigil) {
    paletteScope.value = sigil.id as ScopeId
    paletteQuery.value = value.slice(sigil.sigil!.length)
    // The bound value may not change (the query stays ''), so Vue won't
    // patch the input; write the stripped text back by hand.
    if (input.value) input.value.value = paletteQuery.value
    return
  }
  paletteQuery.value = value
}

/** Only a pointer that moved selects, so rows sliding under a still mouse don't. */
function onPointer(event: MouseEvent, row: PaletteRow) {
  if (event.clientX === lastPointer.x && event.clientY === lastPointer.y) return
  lastPointer = { x: event.clientX, y: event.clientY }
  selectedId.value = row.id
}

function pieces(row: PaletteRow): { text: string; hit: boolean }[] {
  const hits = titleMatches(row, paletteQuery.value)
  const out: { text: string; hit: boolean }[] = []
  ;[...row.title].forEach((char, i) => {
    const hit = hits.has(i)
    const last = out[out.length - 1]
    if (last && last.hit === hit) last.text += char
    else out.push({ text: char, hit })
  })
  return out
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="paletteOpen"
      class="fixed inset-0 z-50 flex items-start justify-center bg-neutral/40 px-4 pt-24"
      @mousedown.self="closePalette"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        class="flex max-h-128 w-full max-w-xl flex-col overflow-hidden rounded-box border border-base-300 bg-base-100 shadow-xl"
        @keydown="onKeydown"
      >
        <div class="flex items-center gap-2 border-b border-base-300 px-3">
          <Search class="size-4 shrink-0 text-base-content/70" aria-hidden="true" />
          <input
            ref="input"
            :value="paletteQuery"
            role="combobox"
            aria-autocomplete="list"
            aria-expanded="true"
            :aria-controls="listId"
            :aria-activedescendant="
              selectedId && ordered.length > 0 ? `${listId}-${selectedId}` : undefined
            "
            aria-label="Search"
            :placeholder="scope.placeholder"
            class="h-12 w-full bg-transparent text-sm outline-none"
            @input="onInput"
          />
        </div>
        <div
          role="tablist"
          aria-label="Scope"
          class="flex gap-1 border-b border-base-300 px-2 py-1.5"
        >
          <button
            v-for="s in SCOPES"
            :key="s.id"
            type="button"
            role="tab"
            tabindex="-1"
            :aria-selected="paletteScope === s.id"
            class="btn btn-ghost btn-xs"
            :class="paletteScope === s.id ? 'bg-base-200' : 'text-base-content/70'"
            @mousedown.prevent
            @click="paletteScope = s.id"
          >
            {{ s.label }}
            <kbd v-if="s.sigil" class="kbd kbd-xs">{{ s.sigil }}</kbd>
          </button>
        </div>
        <ul :id="listId" role="listbox" aria-label="Results" class="overflow-y-auto p-1.5">
          <template v-for="(section, index) in sections" :key="`${index}-${section.label}`">
            <li
              role="presentation"
              class="px-2.5 pt-2 pb-1 text-xs font-medium text-base-content/70"
            >
              {{ section.label }}
            </li>
            <li
              v-for="row in section.rows"
              :id="optionId(row)"
              :key="`${section.label}-${row.id}`"
              role="option"
              :aria-selected="row.id === selectedId"
              class="flex cursor-pointer items-center gap-2.5 rounded-field px-2.5 py-2 text-sm"
              :class="row.id === selectedId ? 'bg-base-200' : ''"
              @mousemove="onPointer($event, row)"
              @mousedown.prevent
              @click="runRow(row)"
            >
              <component
                :is="row.icon"
                v-if="row.icon"
                class="size-4 shrink-0 text-base-content/70"
                aria-hidden="true"
              />
              <span class="min-w-0 flex-1 truncate">
                <span v-if="section.paths && row.group" class="text-base-content/70"
                  >{{ row.group }} ›
                </span>
                <template v-for="(piece, i) in pieces(row)" :key="i">
                  <mark v-if="piece.hit" class="bg-transparent font-semibold text-base-content">{{
                    piece.text
                  }}</mark>
                  <template v-else>{{ piece.text }}</template>
                </template>
              </span>
              <span v-if="row.kind" class="text-xs text-base-content/70">{{ row.kind }}</span>
              <CornerDownLeft
                v-if="row.id === selectedId"
                class="size-3.5 text-base-content/70"
                aria-hidden="true"
              />
            </li>
          </template>
        </ul>
        <p
          v-if="ordered.length === 0"
          class="px-4 py-8 text-center text-sm text-base-content/70"
          role="status"
        >
          {{ paletteQuery ? `No results for “${paletteQuery}”` : 'Nothing here yet.' }}
        </p>
        <div
          class="flex gap-4 border-t border-base-300 px-3 py-2 text-xs text-base-content/70"
          aria-hidden="true"
        >
          <span><kbd class="kbd kbd-xs">↑↓</kbd> move</span>
          <span><kbd class="kbd kbd-xs">↵</kbd> open</span>
          <span><kbd class="kbd kbd-xs">⇥</kbd> scope</span>
          <span><kbd class="kbd kbd-xs">esc</kbd> close</span>
        </div>
      </div>
    </div>
  </Teleport>
</template>
