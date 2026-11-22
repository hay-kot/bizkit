import { describe, expect, it } from 'vitest'
import { nextTick } from 'vue'

import { useSlugFromName } from './useSlugFromName'

describe('useSlugFromName', () => {
  it('follows the name until the slug is edited', async () => {
    const form = useSlugFromName()
    form.name.value = 'Acme Corp'
    await nextTick()
    expect(form.slug.value).toBe('acme-corp')
    form.onSlugInput()
    form.slug.value = 'custom'
    form.name.value = 'Other'
    await nextTick()
    expect(form.slug.value).toBe('custom')
    form.reset()
    expect([form.name.value, form.slug.value]).toEqual(['', ''])
  })
})
