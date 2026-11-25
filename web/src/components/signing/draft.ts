/**
 * A draft request's signers and fields as the sender edits them. Signers
 * added on the page have no ID until `signers.put` gives them one, so
 * fields point at signers by `key`: the ID for saved signers, a local key
 * for new ones.
 */
import type {
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
 * The fields as `fields.put` takes them. `signerIds` maps signer keys to
 * IDs; a field whose signer has none is left out, which only happens if a
 * signer save failed halfway.
 */
export function fieldInputs(
  fields: FieldDraft[],
  signerIds: Map<string, string>,
): SigningFieldInput[] {
  return fields.flatMap((field) => {
    const signerId = signerIds.get(field.signerKey)
    if (!signerId) return []
    return [
      {
        signer_id: signerId,
        kind: field.kind,
        page: field.page,
        x: field.x,
        y: field.y,
        width: field.width,
        height: field.height,
        required: mayBeOptional(field.kind) ? field.required : true,
        label: mayBeOptional(field.kind) ? field.label.trim() : '',
      },
    ]
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
