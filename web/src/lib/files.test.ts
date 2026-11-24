import { describe, expect, it } from 'vitest'

import { fileToBase64, shortFileName } from './files'

describe('fileToBase64', () => {
  it('encodes bytes as standard base64', async () => {
    const blob = new Blob([new Uint8Array([0x25, 0x50, 0x44, 0x46, 0xff, 0x00])])
    expect(await fileToBase64(blob)).toBe('JVBERv8A')
  })

  it('encodes files larger than one chunk', async () => {
    const bytes = new Uint8Array(0x8000 * 2 + 3).fill(0x41)
    const encoded = await fileToBase64(new Blob([bytes]))
    expect(atob(encoded)).toHaveLength(bytes.length)
  })
})

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
