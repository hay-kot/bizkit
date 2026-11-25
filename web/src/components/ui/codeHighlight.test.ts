import { describe, expect, it } from 'vitest'

import { highlight, type CodeLanguage } from './codeHighlight'

function classOf(runs: { text: string; class: string }[], text: string) {
  return runs.find((run) => run.text === text)?.class
}

describe('highlight', () => {
  it.each<[CodeLanguage, string, string, string]>([
    ['javascript', "const a = 'x' // note", "'x'", 'text-secondary'],
    ['typescript-type', '(url: string) => Promise<Response>', 'url', 'text-code-property'],
    ['json', '{"a": 1}', '1', 'text-code-number'],
    ['html', '<script src="s.js"></script>', 'src', 'text-code-property'],
    ['shell', "curl -X POST 'https://x' # send", '# send', 'text-code-comment italic'],
  ])('colors %s tokens', async (language, code, token, expected) => {
    const runs = await highlight(code, language)
    expect(runs.map((run) => run.text).join('')).toBe(code)
    expect(classOf(runs, token)).toBe(expected)
  })

  it('highlights the script inside HTML', async () => {
    const runs = await highlight('<script>const a = 1</script>', 'html')
    expect(classOf(runs, 'const')).toBe('text-primary')
  })

  it('keeps line breaks and leaves text plain', async () => {
    const code = '{\n  "a": true\n}'
    expect((await highlight(code, 'json')).map((run) => run.text).join('')).toBe(code)
    expect(await highlight('a <b>', 'text')).toEqual([{ text: 'a <b>', class: '' }])
  })
})
