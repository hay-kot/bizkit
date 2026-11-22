import { describe, expect, it } from 'vitest'

import { errorCode, errorMessage } from './errors'

describe('errorMessage', () => {
  it('reads the API message', () => {
    expect(errorMessage({ error: { code: 'conflict', message: 'the slug acme is taken' } })).toBe(
      'the slug acme is taken',
    )
  })

  it('prefers field errors', () => {
    const error = {
      error: {
        code: 'validation',
        message: 'invalid input',
        fields: { slug: ['must be 2 to 48 characters'] },
      },
    }
    expect(errorMessage(error)).toBe('slug must be 2 to 48 characters')
  })

  it('falls back for unknown errors', () => {
    expect(errorMessage(42)).toBe('Something went wrong. Try again.')
    expect(errorCode(42)).toBeUndefined()
  })
})
