import { reactive } from 'vue'

export type ToastKind = 'success' | 'error' | 'info'

export interface Toast {
  id: number
  kind: ToastKind
  message: string
}

let nextId = 1
export const toasts = reactive<Toast[]>([])

/** Shows a short message in the corner; it goes away on its own. */
export function toast(message: string, kind: ToastKind = 'success', ms = 3500) {
  const id = nextId++
  toasts.push({ id, kind, message })
  window.setTimeout(() => dismissToast(id), ms)
}

export function dismissToast(id: number) {
  const index = toasts.findIndex((t) => t.id === id)
  if (index >= 0) toasts.splice(index, 1)
}
