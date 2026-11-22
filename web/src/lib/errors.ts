/** The shape of every API error body. */
interface ErrorBodyLike {
  error: { code: string; message: string; fields?: Record<string, string[]> | null }
}

function isErrorBody(value: unknown): value is ErrorBodyLike {
  return (
    typeof value === 'object' &&
    value !== null &&
    'error' in value &&
    typeof (value as ErrorBodyLike).error?.message === 'string'
  )
}

/** A one-line message for any error the API client throws. */
export function errorMessage(error: unknown): string {
  if (isErrorBody(error)) {
    const fields = error.error.fields
    if (fields) {
      const parts = Object.entries(fields).map(([field, messages]) =>
        field ? `${field} ${messages.join(', ')}` : messages.join(', '),
      )
      if (parts.length > 0) return parts.join('; ')
    }
    return error.error.message
  }
  if (error instanceof Error) return error.message
  return 'Something went wrong. Try again.'
}

/** The API error code, such as `conflict`, if `error` is an API error. */
export function errorCode(error: unknown): string | undefined {
  return isErrorBody(error) ? error.error.code : undefined
}
