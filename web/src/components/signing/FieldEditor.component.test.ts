import { mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { describe, expect, it } from 'vitest'

import type { FieldDraft, SignerDraft } from './draft'
import FieldEditor from './FieldEditor.vue'

/** PdfPages without pdf.js: one 600 by 800 page with its slot. */
const OnePage = defineComponent({
  setup(_, { slots }) {
    return () => h('div', slots.page?.({ page: 1, width: 600, height: 800 }))
  },
})

const signers: SignerDraft[] = [
  { key: 'ada', id: 'ada', name: 'Ada', email: 'ada@example.com', signing_order: 1 },
  { key: 'signer-9', name: 'Bo', email: 'bo@example.com', signing_order: 2 },
]
const field: FieldDraft = {
  key: 'field-3',
  signerKey: 'signer-9',
  kind: 'signature',
  page: 1,
  x: 0.1,
  y: 0.1,
  width: 0.3,
  height: 0.05,
  required: true,
  label: '',
}

function mountEditor(props: {
  signers: SignerDraft[]
  modelValue: FieldDraft[]
  locked?: boolean
}) {
  return mount(FieldEditor, {
    props: { url: '/doc.pdf', pageCount: 1, ...props },
    global: { stubs: { PdfPages: OnePage } },
    attachTo: document.body,
  })
}

describe('FieldEditor', () => {
  it('keeps the signer picked for new fields and the selection through a save', async () => {
    const wrapper = mountEditor({ signers, modelValue: [field] })
    wrapper.vm.forSigner = 'signer-9'
    wrapper.vm.selectedKey = 'field-3'

    // What the page does after a save: new keys become IDs.
    const ids = new Map([
      ['signer-9', 'bo-id'],
      ['field-3', 'field-id'],
    ])
    wrapper.vm.remap(ids)
    await wrapper.setProps({
      signers: [signers[0]!, { ...signers[1]!, key: 'bo-id', id: 'bo-id' }],
      modelValue: [{ ...field, key: 'field-id', signerKey: 'bo-id' }],
    })

    expect(wrapper.vm.forSigner).toBe('bo-id')
    expect(wrapper.vm.selectedKey).toBe('field-id')
    expect(wrapper.get('[data-field="field-id"]').attributes('aria-pressed')).toBe('true')
    wrapper.unmount()
  })

  it('changes nothing while locked', async () => {
    const wrapper = mountEditor({ signers, modelValue: [field], locked: true })
    await wrapper.get('[data-field="field-3"]').trigger('keydown', { key: 'ArrowRight' })
    await wrapper.get('[data-field="field-3"]').trigger('keydown', { key: 'Delete' })
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    expect(wrapper.get('fieldset').attributes('disabled')).toBeDefined()
    wrapper.unmount()
  })
})
