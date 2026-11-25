import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'

import CodeBlock from './CodeBlock.vue'

// The highlighter and the language parser load through dynamic imports.
async function settle(wrapper: VueWrapper) {
  await vi.waitFor(() => expect(wrapper.find('pre span').exists()).toBe(true))
  await flushPromises()
}

describe('CodeBlock', () => {
  it('shows the text at once, then colors it', async () => {
    const wrapper = mount(CodeBlock, { props: { code: 'const a = 1', language: 'javascript' } })
    expect(wrapper.get('pre').text()).toBe('const a = 1')
    await settle(wrapper)
    expect(wrapper.get('span.text-primary').text()).toBe('const')
    expect(wrapper.get('pre').text()).toBe('const a = 1')
  })

  it('renders markup in the code as text', async () => {
    const code = '<img src=x onerror="alert(1)">'
    const wrapper = mount(CodeBlock, { props: { code, language: 'html' } })
    await settle(wrapper)
    expect(wrapper.find('img').exists()).toBe(false)
    expect(wrapper.get('pre').text()).toBe(code)
  })

  it('follows a new code value', async () => {
    const wrapper = mount(CodeBlock, { props: { code: '1', language: 'json' } })
    await settle(wrapper)
    await wrapper.setProps({ code: 'true' })
    await vi.waitFor(() => expect(wrapper.find('span.text-code-number').text()).toBe('true'))
  })

  it('adds a copy button and passes attributes to the block', () => {
    const wrapper = mount(CodeBlock, {
      props: { code: 'x', language: 'text', copy: 'Copy snippet' },
      attrs: { 'data-testid': 'snippet' },
    })
    expect(wrapper.get('button').attributes('aria-label')).toBe('Copy snippet')
    expect(wrapper.get('pre').attributes('data-testid')).toBe('snippet')
  })
})
