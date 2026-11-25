import { describe, expect, it } from 'vitest'

import { shortFileName, withShortName } from './files'

describe('shortFileName', () => {
  it('keeps short names', () => {
    expect(shortFileName('contract.pdf')).toBe('contract.pdf')
  })

  it('cuts long names and keeps the extension', () => {
    const cut = shortFileName(`${'a'.repeat(250)}.pdf`)
    expect(cut).toHaveLength(200)
    expect(cut.endsWith('.pdf')).toBe(true)
  })
})

describe('withShortName', () => {
  it('keeps a file whose name fits', () => {
    const file = new File(['%PDF-'], 'contract.pdf', { type: 'application/pdf' })
    expect(withShortName(file)).toBe(file)
  })

  it('renames a file whose name is too long, keeping its bytes and type', async () => {
    const file = new File(['%PDF-'], `${'a'.repeat(250)}.pdf`, { type: 'application/pdf' })
    const renamed = withShortName(file)
    expect(renamed.name).toHaveLength(200)
    expect(renamed.type).toBe('application/pdf')
    expect(await renamed.text()).toBe('%PDF-')
  })
})
