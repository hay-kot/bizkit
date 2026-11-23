import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { defineComponent, h } from 'vue'

import JsonEditor from './JsonEditor.vue'

// CodeMirror needs layout APIs that happy-dom lacks, and the behavior under
// test is all in JsonEditor, so a plain stand-in replaces the editor.
const CodeEditorStub = defineComponent({
  props: { modelValue: { type: String, required: true }, label: { type: String, required: true } },
  setup: (props) => () => h('pre', { 'aria-label': props.label }, props.modelValue),
})

function mountEditor(modelValue: string) {
  const wrapper = mount(JsonEditor, {
    props: {
      label: 'Input',
      modelValue,
      'onUpdate:modelValue': (value: string) => wrapper.setProps({ modelValue: value }),
    },
    global: { stubs: { CodeEditor: CodeEditorStub } },
  })
  return wrapper
}

describe('JsonEditor', () => {
  it('formats valid JSON with two-space indent', async () => {
    const wrapper = mountEditor('{"a":1,"b":[true,null]}')
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
    expect(wrapper.vm.parsed).toEqual({ ok: true, value: { a: 1, b: [true, null] } })
    await wrapper.get('button').trigger('click')
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([
      '{\n  "a": 1,\n  "b": [\n    true,\n    null\n  ]\n}',
    ])
  })

  it('shows a parse error and leaves invalid text alone', async () => {
    const wrapper = mountEditor('{"a":')
    expect(wrapper.get('[role="alert"]').text()).toContain('Invalid JSON')
    expect(wrapper.vm.parsed.ok).toBe(false)
    expect(wrapper.get('button').attributes('disabled')).toBeDefined()

    await wrapper.setProps({ modelValue: '[1]' })
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
  })
})
