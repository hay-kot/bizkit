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
let task: PDFDocumentLoadingTask | undefined
const proxies = shallowRef<PDFPageProxy[]>([])
let width = 0
let generation = 0

function canvasRef(number: number) {
  return (el: unknown) => {
    if (el instanceof HTMLCanvasElement) canvases.set(number, el)
    else canvases.delete(number)
  }
}

/**
 * Lays every page out at `width`, then draws them one after another, so a
 * long document doesn't hold every page's bitmap in flight at once. A newer
 * layout (a resize) stops an older one between pages.
 */
async function layout() {
  const run = ++generation
  if (!width || proxies.value.length === 0) return
  pages.value = proxies.value.map((page) => {
    const viewport = page.getViewport({ scale: scaleToWidth(page, width) })
    return { number: page.pageNumber, width: viewport.width, height: viewport.height }
  })
  // The canvases mount with the new sizes on the next render.
  await new Promise((resolve) => requestAnimationFrame(resolve))
  for (const page of proxies.value) {
    if (run !== generation) return
    const canvas = canvases.get(page.pageNumber)
    if (!canvas) continue
    const viewport = page.getViewport({ scale: scaleToWidth(page, width) })
    try {
      await renderPage(page, canvas, viewport).promise
    } catch (error) {
      if ((error as { name?: string })?.name !== 'RenderingCancelledException') failed.value = error
    }
  }
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
    loading.value = true
    failed.value = undefined
    pages.value = []
    proxies.value = []
    await task?.destroy()
    try {
      task = await loadPdf(url)
      const loaded = await task.promise
      const numbers = Array.from({ length: loaded.numPages }, (_, i) => i + 1)
      proxies.value = await Promise.all(numbers.map((n) => loaded.getPage(n)))
      await layout()
    } catch (error) {
      failed.value = error
    } finally {
      loading.value = false
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  generation++
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
