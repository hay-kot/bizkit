<script setup lang="ts">
import { useDebounceFn, useResizeObserver } from '@vueuse/core'
import { onBeforeUnmount, ref, shallowRef, watch } from 'vue'

import ErrorAlert from '@/components/ErrorAlert.vue'
import {
  loadPdf,
  renderPage,
  scaleToWidth,
  type PDFDocumentLoadingTask,
  type PDFPageProxy,
  type RenderTask,
} from '@/lib/pdf'

/**
 * Every page of the PDF at `url`, each drawn to a canvas as wide as the
 * column. The `#page` slot sits over each page, for `FieldBox`es; it gets
 * the page number (from 1) and the page's size in CSS pixels.
 */
const props = defineProps<{ url: string; title?: string }>()
defineSlots<{ page(props: { page: number; width: number; height: number }): unknown }>()

interface Shown {
  number: number
  width: number
  height: number
}

const root = ref<HTMLElement>()
const pages = ref<Shown[]>([])
const failed = ref<unknown>()
const loading = ref(true)
const canvases = new Map<number, HTMLCanvasElement>()
const proxies = shallowRef<PDFPageProxy[]>([])
let task: PDFDocumentLoadingTask | undefined
let width = 0
/** Bumped by every layout, so an older one stops between pages. */
let layoutRun = 0
/** Bumped by every load and on unmount, so a late load is thrown away. */
let loadRun = 0
let unmounted = false

/**
 * The render in flight on each canvas. pdf.js refuses a second render on a
 * canvas while one runs, so a relayout cancels the old one first.
 */
const rendering = new Map<HTMLCanvasElement, RenderTask>()

function cancelRenders() {
  for (const render of rendering.values()) render.cancel()
  rendering.clear()
}

function canvasRef(number: number) {
  return (el: unknown) => {
    if (el instanceof HTMLCanvasElement) canvases.set(number, el)
    else canvases.delete(number)
  }
}

const isCancel = (error: unknown) =>
  (error as { name?: string } | null)?.name === 'RenderingCancelledException'

/**
 * Lays every page out at `width`, then draws them one after another, so a
 * long document doesn't hold every page's bitmap in flight at once. A newer
 * layout (a resize) cancels the renders of an older one.
 */
async function layout() {
  const run = ++layoutRun
  cancelRenders()
  if (!width || proxies.value.length === 0) return
  pages.value = proxies.value.map((page) => {
    const viewport = page.getViewport({ scale: scaleToWidth(page, width) })
    return { number: page.pageNumber, width: viewport.width, height: viewport.height }
  })
  // The canvases mount with the new sizes on the next render.
  await new Promise((resolve) => requestAnimationFrame(resolve))
  let ok = true
  for (const page of proxies.value) {
    if (run !== layoutRun) return
    const canvas = canvases.get(page.pageNumber)
    if (!canvas) continue
    const viewport = page.getViewport({ scale: scaleToWidth(page, width) })
    const render = renderPage(page, canvas, viewport)
    rendering.set(canvas, render)
    try {
      await render.promise
    } catch (error) {
      if (isCancel(error)) return
      ok = false
      failed.value = error
    } finally {
      if (rendering.get(canvas) === render) rendering.delete(canvas)
    }
  }
  if (ok && run === layoutRun) failed.value = undefined
}

const relayout = useDebounceFn(layout, 150)

useResizeObserver(root, (entries) => {
  const next = Math.floor(entries[0]?.contentRect.width ?? 0)
  if (next === width || next === 0) return
  const first = width === 0
  width = next
  if (first) void layout()
  else void relayout()
})

watch(
  () => props.url,
  async (url) => {
    const run = ++loadRun
    layoutRun++
    cancelRenders()
    loading.value = true
    failed.value = undefined
    pages.value = []
    proxies.value = []
    const previous = task
    task = undefined
    await previous?.destroy()
    let mine: PDFDocumentLoadingTask | undefined
    try {
      mine = await loadPdf(url)
      // A newer load, or leaving the page, makes this one moot.
      if (run !== loadRun || unmounted) {
        await mine.destroy()
        return
      }
      task = mine
      const loaded = await mine.promise
      const numbers = Array.from({ length: loaded.numPages }, (_, i) => i + 1)
      const loadedPages = await Promise.all(numbers.map((n) => loaded.getPage(n)))
      if (run !== loadRun || unmounted) return
      proxies.value = loadedPages
      await layout()
    } catch (error) {
      if (run === loadRun && !unmounted) failed.value = error
    } finally {
      if (run === loadRun) loading.value = false
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  unmounted = true
  loadRun++
  layoutRun++
  cancelRenders()
  void task?.destroy()
})
</script>

<template>
  <div ref="root" class="flex flex-col gap-4">
    <ErrorAlert
      v-if="failed"
      :error="new Error('The document could not be shown. Reload the page to try again.')"
    />
    <div
      v-if="loading && pages.length === 0"
      class="aspect-3/4 w-full animate-pulse rounded-box border border-base-300 bg-base-300"
      aria-busy="true"
      aria-label="Loading the document"
    ></div>
    <section
      v-for="page in pages"
      :key="page.number"
      class="relative overflow-hidden rounded-box border border-base-300 bg-base-100 shadow-sm"
      :aria-label="`${title ? `${title}, page` : 'Page'} ${page.number} of ${pages.length}`"
    >
      <canvas
        :ref="canvasRef(page.number)"
        :width="Math.floor(page.width)"
        :height="Math.floor(page.height)"
        class="block h-auto w-full"
        aria-hidden="true"
      ></canvas>
      <slot name="page" :page="page.number" :width="page.width" :height="page.height" />
    </section>
  </div>
</template>
