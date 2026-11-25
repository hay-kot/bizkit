import { describe, expect, it } from 'vitest'

import type {
  DraftApi,
  SignerSlot,
  SigningFieldInput,
  SigningRequestDetail,
} from '@/queries/signing'

import { type FieldDraft, remapKey, saveDraft, type SignerDraft } from './draft'

/**
 * A fake server for one draft: signers.put and fields.put answer like the
 * real ones, listing rows in the order sent with IDs for new ones.
 */
function fakeApi(initial: { signers: { id: string; name: string; email: string }[] }) {
  let next = 0
  const calls: string[] = []
  let signers = initial.signers.map((s) => ({ ...s, signing_order: 1, signed: false }))
  let fields: (SigningFieldInput & { id: string })[] = []
  const detail = (): SigningRequestDetail =>
    ({
      request: { signers },
      fields: fields.map((f) => ({ ...f, required: f.required ?? true, label: f.label ?? '' })),
      events: [],
    }) as unknown as SigningRequestDetail
  const api: DraftApi = {
    async putSigners(slots: SignerSlot[]) {
      calls.push('signers')
      signers = slots.map((slot) => ({
        id: slot.id ?? `signer-id-${++next}`,
        name: slot.name,
        email: slot.email,
        signing_order: slot.signing_order ?? 1,
        signed: false,
      }))
      const kept = new Set(signers.map((s) => s.id))
      fields = fields.filter((f) => kept.has(f.signer_id))
      return detail()
    },
    async putFields(inputs: SigningFieldInput[]) {
      calls.push(`fields:${inputs.length}`)
      const known = new Set(signers.map((s) => s.id))
      for (const input of inputs) {
        if (!known.has(input.signer_id)) throw new Error(`unknown signer ${input.signer_id}`)
      }
      fields = inputs.map((input) => ({ ...input, id: `field-id-${++next}` }))
      return detail()
    },
  }
  return { api, calls, fields: () => fields }
}

const saved = (id: string, name: string): SignerDraft => ({
  key: id,
  id,
  name,
  email: `${name.toLowerCase()}@example.com`,
  signing_order: 1,
})

function field(key: string, signerKey: string, kind: FieldDraft['kind'] = 'signature'): FieldDraft {
  return {
    key,
    signerKey,
    kind,
    page: 1,
    x: 0.1,
    y: 0.1,
    width: 0.2,
    height: 0.05,
    required: true,
    label: '',
  }
}

describe('saving a draft', () => {
  it('gives fields of a new signer that signer once it has an ID', async () => {
    const server = fakeApi({ signers: [{ id: 'ada', name: 'Ada', email: 'ada@example.com' }] })
    const signers = [
      saved('ada', 'Ada'),
      { key: 'signer-new', name: 'Bo', email: 'bo@example.com', signing_order: 2 },
    ]
    const fields = [
      field('f1', 'ada'),
      field('f2', 'signer-new'),
      field('f3', 'signer-new', 'date'),
    ]

    const result = await saveDraft(server.api, signers, fields, true)

    expect(server.calls).toEqual(['signers', 'fields:3'])
    const bo = result.detail.request.signers[1]!.id
    expect(result.ids.get('signer-new')).toBe(bo)
    expect(result.detail.fields.map((f) => f.signer_id)).toEqual(['ada', bo, bo])
    for (const local of fields) {
      const id = result.ids.get(local.key)
      const stored = result.detail.fields.find((f) => f.id === id)
      expect(stored?.signer_id, local.key).toBe(remapKey(local.signerKey, result.ids))
      expect(stored?.kind).toBe(local.kind)
    }
  })

  it('saves only the fields, and keeps every one, when the signers are unchanged', async () => {
    const server = fakeApi({
      signers: [
        { id: 'ada', name: 'Ada', email: 'ada@example.com' },
        { id: 'bo', name: 'Bo', email: 'bo@example.com' },
      ],
    })
    const signers = [saved('ada', 'Ada'), saved('bo', 'Bo')]
    const fields = [field('f1', 'ada'), field('f2', 'bo'), field('f3', 'bo', 'checkbox')]

    const result = await saveDraft(server.api, signers, fields, false)

    expect(server.calls).toEqual(['fields:3'])
    expect(server.fields().map((f) => f.signer_id)).toEqual(['ada', 'bo', 'bo'])
    expect(fields.every((f) => result.ids.has(f.key))).toBe(true)
  })

  it('sends the lists as they were when the save started', async () => {
    const server = fakeApi({ signers: [] })
    const signers: SignerDraft[] = [
      { key: 'signer-a', name: 'Ada', email: 'ada@example.com', signing_order: 1 },
    ]
    const fields = [field('f1', 'signer-a')]

    const pending = saveDraft(server.api, signers, fields, true)
    signers.unshift({ key: 'signer-b', name: 'Bo', email: 'bo@example.com', signing_order: 1 })
    fields.push(field('f2', 'signer-b'))
    const result = await pending

    expect(server.calls).toEqual(['signers', 'fields:1'])
    expect(result.detail.request.signers.map((s) => s.name)).toEqual(['Ada'])
    expect(result.ids.get('signer-a')).toBe(result.detail.request.signers[0]!.id)
    expect(result.ids.has('signer-b')).toBe(false)
  })

  it('refuses to send fields whose signer has no ID rather than drop them', async () => {
    const server = fakeApi({ signers: [{ id: 'ada', name: 'Ada', email: 'ada@example.com' }] })
    await expect(
      saveDraft(server.api, [saved('ada', 'Ada')], [field('f1', 'signer-gone')], false),
    ).rejects.toThrow(/no saved signer/)
    expect(server.calls).toEqual([])
  })
})

describe('remapKey', () => {
  it('maps local keys and keeps IDs', () => {
    const ids = new Map([['signer-1', 'abc']])
    expect(remapKey('signer-1', ids)).toBe('abc')
    expect(remapKey('abc', ids)).toBe('abc')
  })
})
