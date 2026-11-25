/**
 * The one door to pdf.js: loading a PDF, rendering a page to a canvas, and
 * the coordinate math between a rendered page and the page fractions that
 * signing fields use.
 *
 * Field rects are fractions of the page as displayed, after `/Rotate`, with
 * a top-left origin. That is a pdf.js viewport at any scale divided by its
 * width and height, so the client never needs the page's size in points.
 */
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url'
import type {
  PDFDocumentLoadingTask,
  PDFDocumentProxy,
  PDFPageProxy,
  PageViewport,
  RenderTask,
} from 'pdfjs-dist'

export type { PDFDocumentLoadingTask, PDFDocumentProxy, PDFPageProxy, PageViewport, RenderTask }

/** A rect in page fractions: 0..1 from the top-left corner. */
export interface Rect {
  x: number
  y: number
  width: number
  height: number
}

/** A viewport's size, in CSS pixels. */
export interface Size {
  width: number
  height: number
}

/** The part of a pdf.js `PageViewport` the point helpers use. */
export interface ViewportLike extends Size {
  convertToViewportPoint(x: number, y: number): number[]
  convertToPdfPoint(x: number, y: number): number[]
}

// pdf.js is large and only signing pages need it, so it loads on first use.
let library: Promise<typeof import('pdfjs-dist')> | undefined

function pdfjs() {
  library ??= import('pdfjs-dist').then((lib) => {
    lib.GlobalWorkerOptions.workerSrc = workerUrl
    return lib
  })
  return library
}

/** Where the build serves pdf.js's data files (see `pdfjsAssets` in vite.config.ts). */
export function assetBase(version: string): string {
  return `/pdfjs/${version}/`
}

/**
 * Opens the PDF at `url` (same origin); `destroy()` on the task frees it.
 *
 * The PDF comes from a stranger, so it only ever draws: no XFA forms, no
 * annotation, text or scripting layers, and `pdf.sandbox` is never loaded.
 * pdf.js 6 removed `isEvalSupported` and the document-level
 * `enableScripting` option along with the code paths they guarded (fonts no
 * longer compile to functions), so there is nothing left to switch off
 * there; 6.2.108 or later is needed for CVE-2026-16633.
 *
 * The CMaps let CJK text draw, and the wasm decoders JPX and JBIG2 images
 * (scans) and ICC colors. `standardFontDataUrl` stays unset: pdf.js's
 * `standard_fonts/` holds Liberation fonts under SIL OFL 1.1, a license
 * bizkit doesn't ship, so PDFs that don't embed the 14 standard fonts draw
 * them with the browser's own fonts instead.
 */
export async function loadPdf(url: string): Promise<PDFDocumentLoadingTask> {
  const lib = await pdfjs()
  const base = assetBase(lib.version)
  return lib.getDocument({
    url,
    withCredentials: false,
    enableXfa: false,
    cMapUrl: `${base}cmaps/`,
    cMapPacked: true,
    wasmUrl: `${base}wasm/`,
  })
}

/** The scale that makes `page` `width` CSS pixels wide, as displayed. */
export function scaleToWidth(page: PDFPageProxy, width: number): number {
  const natural = page.getViewport({ scale: 1 })
  return natural.width > 0 ? width / natural.width : 1
}

/**
 * Draws `page` into `canvas` at `viewport`, sharp on high-density screens.
 * The canvas keeps its CSS size from its classes; only its backing store
 * grows with the device pixel ratio.
 */
export function renderPage(
  page: PDFPageProxy,
  canvas: HTMLCanvasElement,
  viewport: PageViewport,
): RenderTask {
  const ratio = window.devicePixelRatio || 1
  canvas.width = Math.floor(viewport.width * ratio)
  canvas.height = Math.floor(viewport.height * ratio)
  return page.render({
    canvas,
    viewport,
    transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0],
  })
}

const clamp01 = (value: number) => Math.min(1, Math.max(0, value))

/** A rect in viewport pixels as page fractions, cut to the page. */
export function toFractions(rect: Rect, size: Size): Rect {
  const left = clamp01(rect.x / size.width)
  const top = clamp01(rect.y / size.height)
  const right = clamp01((rect.x + rect.width) / size.width)
  const bottom = clamp01((rect.y + rect.height) / size.height)
  return { x: left, y: top, width: Math.max(0, right - left), height: Math.max(0, bottom - top) }
}

/** A rect in page fractions as viewport pixels. */
export function toPixels(rect: Rect, size: Size): Rect {
  return {
    x: rect.x * size.width,
    y: rect.y * size.height,
    width: rect.width * size.width,
    height: rect.height * size.height,
  }
}

/** A point in PDF user space as page fractions on the displayed page. */
export function pdfPointToFraction(viewport: ViewportLike, x: number, y: number): [number, number] {
  const [vx = 0, vy = 0] = viewport.convertToViewportPoint(x, y)
  return [vx / viewport.width, vy / viewport.height]
}

/** A point in page fractions on the displayed page as PDF user space. */
export function fractionToPdfPoint(
  viewport: ViewportLike,
  fx: number,
  fy: number,
): [number, number] {
  const [x = 0, y = 0] = viewport.convertToPdfPoint(fx * viewport.width, fy * viewport.height)
  return [x, y]
}
