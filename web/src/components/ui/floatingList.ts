import { useElementBounding, useElementSize, useEventListener, useWindowSize } from '@vueuse/core'
import { computed, type Ref } from 'vue'

const EDGE = 8
const GAP = 4
const MIN_LIST_WIDTH = 224
const FLOATING = '[data-floating]'

/**
 * Coordinates for a list teleported to <body> under its trigger, so table and
 * panel scroll containers can't clip it. It opens upward when there's no room
 * below, is at least 224px (14rem) wide so a short toolbar trigger doesn't
 * open a cramped list, and shifts left to stay inside the viewport, which
 * matters near the right edge and on phones.
 *
 * A list whose trigger sits inside another floating surface (a popover,
 * marked `data-floating`) always opens downward with its height capped to
 * the room below, because opening upward would cover the fields above it.
 *
 * Scrolling anything but the list (or another floating surface) closes it,
 * because the coordinates would go stale; `onClose` runs then, so a caller
 * with more state than `open` can reset it too. With `follow`, and for lists
 * inside a floating surface, a scroll moves it with its trigger instead:
 * phones scroll the page when the keyboard opens, and that must not close a
 * form the user is typing in.
 */
export function useFloatingList(
  trigger: Ref<HTMLElement | undefined>,
  list: Ref<HTMLElement | undefined>,
  open: Ref<boolean>,
  onClose: () => void = () => (open.value = false),
  { follow = false }: { follow?: boolean } = {},
) {
  const anchor = useElementBounding(trigger)
  const viewport = useWindowSize({ includeScrollbar: false })
  const { width: listWidth } = useElementSize(list, undefined, { box: 'border-box' })
  const nested = () => Boolean(trigger.value?.parentElement?.closest(FLOATING))
  const position = computed(() => {
    const below = viewport.height.value - anchor.bottom.value
    const inside = nested()
    const up = !inside && below < 336 && anchor.top.value > below
    const maxWidth = Math.max(0, viewport.width.value - 2 * EDGE)
    const minWidth = Math.min(Math.max(anchor.width.value, MIN_LIST_WIDTH), maxWidth)
    const width = Math.max(listWidth.value, minWidth)
    const left = Math.max(EDGE, Math.min(anchor.left.value, viewport.width.value - EDGE - width))
    return {
      left: `${left}px`,
      minWidth: `${minWidth}px`,
      maxWidth: `${maxWidth}px`,
      ...(up
        ? { bottom: `${viewport.height.value - anchor.top.value + GAP}px` }
        : { top: `${anchor.bottom.value + GAP}px` }),
      // The list classes cap it at max-h-80 (20rem); an inline max-height
      // replaces that cap, so it is repeated here.
      ...(inside ? { maxHeight: `min(20rem, ${Math.max(0, below - GAP - EDGE)}px)` } : {}),
    }
  })
  useEventListener(
    'scroll',
    (event: Event) => {
      if (!open.value || list.value?.contains(event.target as Node)) return
      // A list inside a popover scrolls without moving the popover's anchor.
      if (event.target instanceof Element && event.target.closest(FLOATING)) return
      // A frame later, so a popover this list sits in has moved first.
      if (follow || nested()) requestAnimationFrame(() => anchor.update())
      else onClose()
    },
    { capture: true, passive: true },
  )
  return { position, update: anchor.update }
}
