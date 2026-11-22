import { shallowRef } from 'vue'

export interface ConfirmRequest {
  title: string
  message: string
  confirmLabel: string
  danger: boolean
  resolve: (ok: boolean) => void
}

export const pendingConfirm = shallowRef<ConfirmRequest>()

/** Asks the user to confirm; resolves true when they do. */
export function confirmAction(options: {
  title: string
  message: string
  confirmLabel?: string
  danger?: boolean
}): Promise<boolean> {
  pendingConfirm.value?.resolve(false)
  return new Promise((resolve) => {
    pendingConfirm.value = {
      title: options.title,
      message: options.message,
      confirmLabel: options.confirmLabel ?? 'Confirm',
      danger: options.danger ?? true,
      resolve: (ok) => {
        pendingConfirm.value = undefined
        resolve(ok)
      },
    }
  })
}
