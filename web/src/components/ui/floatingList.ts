import { useElementBounding, useElementSize, useEventListener, useWindowSize } from '@vueuse/core'
import { computed, type Ref } from 'vue'

const EDGE = 8
const MIN_LIST_WIDTH = 224

/**
 * Coordinates for a list teleported to <body> under its trigger, so table and
 * panel scroll containers can't clip it. It opens upward when there's no room
 * below, is at least 224px (14rem) wide so a short toolbar trigger doesn't
 * open a cramped list, and shifts left to stay inside the viewport, which
 * matters near the right edge and on phones. Scrolling anything but the list
 * closes it, because the coordinates would go stale; `onClose` runs then, so
 * a caller with more state than `open` can reset it too.
 */
export function useFloatingList(
  trigger: Ref<HTMLElement | undefined>,
  list: Ref<HTMLElement | undefined>,
  open: Ref<boolean>,
  onClose: () => void = () => (open.value = false),
) {
  const anchor = useElementBounding(trigger)
  const viewport = useWindowSize({ includeScrollbar: false })
  const { width: listWidth } = useElementSize(list, undefined, { box: 'border-box' })
  const position = computed(() => {
    const below = viewport.height.value - anchor.bottom.value
    const up = below < 336 && anchor.top.value > below
    const maxWidth = Math.max(0, viewport.width.value - 2 * EDGE)
    const minWidth = Math.min(Math.max(anchor.width.value, MIN_LIST_WIDTH), maxWidth)
    const width = Math.max(listWidth.value, minWidth)
    const left = Math.max(EDGE, Math.min(anchor.left.value, viewport.width.value - EDGE - width))
    return {
      left: `${left}px`,
      minWidth: `${minWidth}px`,
      maxWidth: `${maxWidth}px`,
      ...(up
        ? { bottom: `${viewport.height.value - anchor.top.value + 4}px` }
        : { top: `${anchor.bottom.value + 4}px` }),
    }
  })
  useEventListener(
    'scroll',
    (event: Event) => {
      if (open.value && !list.value?.contains(event.target as Node)) onClose()
    },
    { capture: true, passive: true },
  )
  return { position, update: anchor.update }
}
