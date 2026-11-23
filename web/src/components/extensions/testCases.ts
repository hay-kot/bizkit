import type { Json } from '@/components/ui/JsonEditor.vue'
import type { TestCase } from '@/queries/extensions'

export type CasesResult = { ok: true; cases: TestCase[] } | { ok: false; error: string }

/** Checks the tests editor's JSON against the case shape the API takes. */
export function parseCases(value: Json): CasesResult {
  if (!Array.isArray(value)) return { ok: false, error: 'Tests must be a JSON array of cases.' }
  const names = new Set<string>()
  const cases: TestCase[] = []
  for (const [index, item] of value.entries()) {
    const at = `Case ${index + 1}`
    if (item === null || typeof item !== 'object' || Array.isArray(item)) {
      return { ok: false, error: `${at} must be an object.` }
    }
    const { name, input, expect, ...rest } = item
    const extra = Object.keys(rest)[0]
    if (extra !== undefined) return { ok: false, error: `${at} has an unknown key "${extra}".` }
    if (typeof name !== 'string' || name.trim() === '') {
      return { ok: false, error: `${at} needs a "name".` }
    }
    if (input === undefined) return { ok: false, error: `${at} needs an "input".` }
    if (names.has(name.trim())) return { ok: false, error: `"${name.trim()}" is used twice.` }
    names.add(name.trim())
    cases.push(expect === undefined ? { name, input } : { name, input, expect })
  }
  return { ok: true, cases }
}

export interface DiffLine {
  kind: 'same' | 'removed' | 'added'
  text: string
}

/** A line diff of two texts, from the longest common subsequence. */
export function lineDiff(before: string, after: string): DiffLine[] {
  const a = before.split('\n')
  const b = after.split('\n')
  const lcs: number[][] = Array.from({ length: a.length + 1 }, () =>
    Array<number>(b.length + 1).fill(0),
  )
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i]![j] =
        a[i] === b[j] ? lcs[i + 1]![j + 1]! + 1 : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!)
    }
  }
  const out: DiffLine[] = []
  let i = 0
  let j = 0
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      out.push({ kind: 'same', text: a[i]! })
      i++
      j++
    } else if (lcs[i + 1]![j]! >= lcs[i]![j + 1]!) {
      out.push({ kind: 'removed', text: a[i++]! })
    } else {
      out.push({ kind: 'added', text: b[j++]! })
    }
  }
  while (i < a.length) out.push({ kind: 'removed', text: a[i++]! })
  while (j < b.length) out.push({ kind: 'added', text: b[j++]! })
  return out
}
