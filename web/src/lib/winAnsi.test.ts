import { describe, expect, it } from 'vitest'

import { isWinAnsi } from './winAnsi'

describe('isWinAnsi', () => {
  it('takes Latin letters, accents and the WinAnsi extras', () => {
    expect(isWinAnsi('José Müller')).toBe(true)
    expect(isWinAnsi('Œuvre “quoted” – €5 ™')).toBe(true)
    expect(isWinAnsi('')).toBe(true)
  })

  it('refuses what the standard fonts lack, and control characters', () => {
    expect(isWinAnsi('Пётр')).toBe(false)
    expect(isWinAnsi('山田')).toBe(false)
    expect(isWinAnsi('Ada 😀')).toBe(false)
    expect(isWinAnsi('Ada\nLovelace')).toBe(false)
    expect(isWinAnsi('\u0081')).toBe(false)
  })
})
