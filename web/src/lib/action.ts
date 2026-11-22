import { confirmAction } from './confirm'
import { errorMessage } from './errors'
import { toast } from './toast'

export interface ActionOptions<T> {
  /** Ask first; the action runs only if the user confirms. */
  confirm?: { title: string; message: string; confirmLabel?: string; danger?: boolean }
  /** The work, usually a mutation's `mutateAsync`. */
  run: () => Promise<T>
  /** The toast on success. */
  success?: string | ((result: T) => string)
}

/**
 * Confirm, run, then toast the result or the error. Returns the result, or
 * `undefined` when the user cancelled or the action failed.
 */
export async function runAction<T>(options: ActionOptions<T>): Promise<T | undefined> {
  if (options.confirm && !(await confirmAction(options.confirm))) return undefined
  try {
    const result = await options.run()
    const message =
      typeof options.success === 'function' ? options.success(result) : options.success
    if (message) toast(message)
    return result
  } catch (error) {
    toast(errorMessage(error), 'error')
    return undefined
  }
}
