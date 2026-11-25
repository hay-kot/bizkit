/**
 * A draft request's signers and fields as the sender edits them. Signers
 * added on the page have no ID until `signers.put` gives them one, so
 * fields point at signers by `key`: the ID for saved signers, a local key
 * for new ones.
 */
import type {
  DraftApi,
  SavedDraft,
  SignerSlot,
  SigningField,
  SigningFieldInput,
  SigningFieldKind,
  SigningRequestDetail,
} from '@/queries/signing'

export interface SignerDraft {
  key: string
  id?: string
  name: string
  email: string
  /** A number input's model: '' while emptied. */
  signing_order: number | string
}

export interface FieldDraft {
  key: string
  signerKey: string
  kind: SigningFieldKind
  page: number
  x: number
  y: number
  width: number
  height: number
  required: boolean
  label: string
}

/** Signature, initials and date fields are filled from the signature itself. */
export function mayBeOptional(kind: SigningFieldKind): boolean {
  return kind === 'text' || kind === 'checkbox'
}

/** What the field asks for, in a word. */
export const KIND_LABELS: Record<SigningFieldKind, string> = {
  signature: 'Signature',
  initials: 'Initials',
  date: 'Date signed',
  text: 'Text',
  checkbox: 'Checkbox',
}

let counter = 0
/** A key for a signer or field that has no ID yet. */
export function localKey(prefix: string): string {
  counter += 1
  return `${prefix}-${counter}`
}

export function orderOf(value: number | string): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 1
}

export function signerDrafts(detail: SigningRequestDetail): SignerDraft[] {
  return detail.request.signers.map((signer) => ({
    key: signer.id,
    id: signer.id,
    name: signer.name,
    email: signer.email,
    signing_order: signer.signing_order,
  }))
}

export function fieldDrafts(fields: SigningField[]): FieldDraft[] {
  return fields.map((field) => ({
    key: field.id,
    signerKey: field.signer_id,
    kind: field.kind,
    page: field.page,
    x: field.x,
    y: field.y,
    width: field.width,
    height: field.height,
    required: field.required,
    label: field.label,
  }))
}

export function signerSlots(signers: SignerDraft[]): SignerSlot[] {
  return signers.map((signer) => ({
    id: signer.id ?? null,
    name: signer.name.trim(),
    email: signer.email.trim(),
    signing_order: orderOf(signer.signing_order),
  }))
}

/**
 * The fields as `fields.put` takes them, in the same order. `signerIds`
 * maps signer keys to IDs; a field whose signer has none throws, since
 * sending the rest would delete it.
 */
export function fieldInputs(
  fields: FieldDraft[],
  signerIds: Map<string, string>,
): SigningFieldInput[] {
  return fields.map((field) => {
    const signerId = signerIds.get(field.signerKey)
    if (!signerId) throw new Error('A field has no saved signer. Reload the page and try again.')
    return {
      signer_id: signerId,
      kind: field.kind,
      page: field.page,
      x: field.x,
      y: field.y,
      width: field.width,
      height: field.height,
      required: mayBeOptional(field.kind) ? field.required : true,
      label: mayBeOptional(field.kind) ? field.label.trim() : '',
    }
  })
}

export function sameSigners(a: SignerDraft[], b: SignerDraft[]): boolean {
  const left = signerSlots(a)
  const right = signerSlots(b)
  return (
    left.length === right.length &&
    left.every(
      (signer, i) =>
        signer.id === right[i]?.id &&
        signer.name === right[i]?.name &&
        signer.email.toLowerCase() === right[i]?.email.toLowerCase() &&
        signer.signing_order === right[i]?.signing_order,
    )
  )
}

export function sameFields(a: FieldDraft[], b: FieldDraft[]): boolean {
  const keys = new Map<string, string>()
  for (const field of [...a, ...b]) keys.set(field.signerKey, field.signerKey)
  const left = fieldInputs(a, keys)
  const right = fieldInputs(b, keys)
  return (
    left.length === right.length &&
    left.every((field, i) => JSON.stringify(field) === JSON.stringify(right[i]))
  )
}

/** The signers with no signature field, whom `send` would refuse. */
export function signersWithoutSignature(
  signers: SignerDraft[],
  fields: FieldDraft[],
): SignerDraft[] {
  const signed = new Set(fields.filter((f) => f.kind === 'signature').map((f) => f.signerKey))
  return signers.filter((signer) => !signed.has(signer.key))
}

/**
 * Saves `signers` and `fields` as they are now: `signers.put` when the
 * signers changed, then `fields.put` with every field. New signers get
 * their IDs from the first call, so fields go second. The lists are
 * copied first and the IDs mapped from the copies by position, which is
 * how the server answers, so the caller's lists can't skew the mapping.
 * The result maps every local signer and field key to its server ID.
 */
export async function saveDraft(
  api: DraftApi,
  signers: readonly SignerDraft[],
  fields: readonly FieldDraft[],
  signersChanged: boolean,
): Promise<SavedDraft> {
  const sentSigners = signers.map((signer) => ({ ...signer }))
  const sentFields = fields.map((field) => ({ ...field }))
  const ids = new Map<string, string>()
  if (signersChanged) {
    const afterSigners = await api.putSigners(signerSlots(sentSigners))
    sentSigners.forEach((signer, i) => {
      const id = afterSigners.request.signers[i]?.id
      if (id) ids.set(signer.key, id)
    })
  } else {
    for (const signer of sentSigners) if (signer.id) ids.set(signer.key, signer.id)
  }
  const detail = await api.putFields(fieldInputs(sentFields, ids))
  sentFields.forEach((field, i) => {
    const id = detail.fields[i]?.id
    if (id) ids.set(field.key, id)
  })
  return { detail, ids }
}

/** `key` after a save: its server ID when it was a local key, else itself. */
export function remapKey(key: string, ids: Map<string, string>): string {
  return ids.get(key) ?? key
}
