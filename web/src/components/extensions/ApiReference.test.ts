import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import ApiReference from './ApiReference.vue'

describe('ApiReference', () => {
  it('shows each entry with its signature, errors and example', () => {
    const wrapper = mount(ApiReference, {
      props: {
        entries: [
          {
            name: 'crypto.randomUUID',
            signature: '() => string',
            summary: 'A random UUID.',
            errors: [],
            example: 'crypto.randomUUID()',
          },
          {
            name: 'ctx.fetch',
            signature: '(url: string) => Promise<Response>',
            summary: 'Sends a request.',
            errors: ['the URL is private'],
            example: "await ctx.fetch('https://example.com')",
          },
        ],
      },
    })
    const items = wrapper.findAll('[aria-label="Script API"] > li')
    expect(items.map((item) => item.get('h4').text())).toEqual(['crypto.randomUUID', 'ctx.fetch'])
    expect(items[0]!.text()).not.toContain('Throws when')
    expect(items[1]!.text()).toContain('Throws when')
    expect(items[1]!.text()).toContain('the URL is private')
    expect(items[1]!.text()).toContain("await ctx.fetch('https://example.com')")
  })
})
