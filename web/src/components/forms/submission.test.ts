import { describe, expect, it } from 'vitest'

import type { FieldDef, Submission } from '@/queries/forms'

import { preview, sender, senderField } from './submission'

function submission(data: Record<string, string>): Submission {
  return { data } as unknown as Submission
}

const field = (name: string, kind: FieldDef['kind'] = 'text'): FieldDef =>
  ({ name, kind }) as FieldDef

describe('senderField', () => {
  it('prefers a field of kind email', () => {
    const fields = [field('name'), field('contact', 'email')]
    expect(senderField(fields, submission({ name: 'Ada', contact: 'ada@x.test' }))).toBe('contact')
  })

  it('takes a field named exactly email or e-mail, then a person name', () => {
    expect(senderField([], submission({ name: 'Ada', 'E-Mail': 'ada@x.test' }))).toBe('E-Mail')
    expect(senderField([], submission({ message: 'Hi', full_name: 'Ada' }))).toBe('full_name')
    expect(senderField([], submission({ firstName: 'Ada' }))).toBe('firstName')
  })

  it('ignores names that only contain email or name', () => {
    const data = { company_name: 'Acme', username: 'ada', email_opt_in: 'yes' }
    expect(senderField([], submission(data))).toBeUndefined()
    expect(sender([], submission(data))).toBe('Anonymous')
  })

  it('skips empty fields', () => {
    expect(senderField([], submission({ email: '', name: 'Ada' }))).toBe('name')
  })
})

describe('preview', () => {
  it('leaves out the sender and keeps three values', () => {
    const data = { name: 'Ada', a: '1', b: '2', c: '3', d: '4' }
    expect(preview([], submission(data))).toBe('1 · 2 · 3')
  })

  it('is empty when the sender is the only filled field', () => {
    expect(preview([], submission({ email: 'ada@x.test', message: '' }))).toBe('')
  })

  it('says (empty) only when no field is filled', () => {
    expect(preview([], submission({ email: '', message: '' }))).toBe('(empty)')
  })

  it('follows the form field order when the form has fields', () => {
    const fields = [field('message'), field('name')]
    const data = { name: 'Ada', topic: 'x', message: 'Hello' }
    expect(preview(fields, submission(data))).toBe('Hello')
  })
})
