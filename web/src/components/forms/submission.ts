import type { FieldDef, FieldValue, Submission } from '@/queries/forms'

/** A field's value as one line of text: choices joined with commas. */
export function fieldText(value: FieldValue | undefined): string {
  if (value === undefined) return ''
  return Array.isArray(value) ? value.join(', ') : value
}

function fieldNames(fields: FieldDef[], submission: Submission): string[] {
  return fields.length ? fields.map((f) => f.name) : Object.keys(submission.data)
}

const EMAIL_NAME = /^e-?mail$/i
const PERSON_NAME = /^(?:name|full_?name|first_?name)$/i

/**
 * The filled field that says who sent the submission: an email field (by
 * kind, or named exactly `email`), else one named exactly like a person's
 * name, else none. Forms are free-form, so this stays a guess, and the
 * names are anchored so `company_name` or `username` never count.
 */
export function senderField(fields: FieldDef[], submission: Submission): string | undefined {
  const filled = fieldNames(fields, submission).filter(
    (name) => fieldText(submission.data[name]).length > 0,
  )
  return (
    fields.find((f) => f.kind === 'email' && filled.includes(f.name))?.name ??
    filled.find((name) => EMAIL_NAME.test(name)) ??
    filled.find((name) => PERSON_NAME.test(name))
  )
}

export function sender(fields: FieldDef[], submission: Submission): string {
  const field = senderField(fields, submission)
  return field ? fieldText(submission.data[field]) : 'Anonymous'
}

/**
 * Up to three filled values other than the sender's, for the inbox row.
 * Empty when the sender is the only filled field; `(empty)` only when no
 * field is filled at all.
 */
export function preview(fields: FieldDef[], submission: Submission): string {
  const values = fieldNames(fields, submission)
    .map((name) => ({ name, value: fieldText(submission.data[name]) }))
    .filter(({ value }) => value.length > 0)
  if (values.length === 0) return '(empty)'
  const skip = senderField(fields, submission)
  return values
    .filter(({ name }) => name !== skip)
    .slice(0, 3)
    .map(({ value }) => value)
    .join(' · ')
}
