/**
 * Whether the PDF's standard fonts can print `text`: the WinAnsi characters
 * the server accepts for typed signatures and text fields (`win_ansi` in
 * crates/bizkit-signing/src/pdf.rs). Keep the two lists the same.
 */
const EXTRA = new Set('€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ')

export function isWinAnsi(text: string): boolean {
  for (const c of text) {
    const code = c.codePointAt(0)!
    if ((code >= 0x20 && code <= 0x7e) || (code >= 0xa0 && code <= 0xff) || EXTRA.has(c)) continue
    return false
  }
  return true
}
