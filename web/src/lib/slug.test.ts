import { describe, expect, it } from 'vitest'

import { slugify } from './slug'

describe('slugify', () => {
  it('makes API-safe slugs', () => {
    expect(slugify('Acme Corp')).toBe('acme-corp')
    expect(slugify('  Café Été! ')).toBe('cafe-ete')
    expect(slugify('42 Things')).toBe('things')
  })
})
