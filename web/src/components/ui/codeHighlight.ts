import { highlightCode, tagHighlighter, tags } from '@lezer/highlight'

/** `typescript-type` is a type expression alone, such as a function signature. */
export type CodeLanguage = 'javascript' | 'typescript-type' | 'json' | 'html' | 'shell' | 'text'

/** A run of code text and the classes that color it ('' for plain text). */
export interface CodeRun {
  text: string
  class: string
}

/**
 * Token colors for the editors and static code blocks alike. Classes rather
 * than inline colors, so both follow the light and dark themes.
 */
export const codeHighlighter = tagHighlighter([
  { tag: [tags.keyword, tags.operatorKeyword, tags.modifier], class: 'text-primary' },
  { tag: [tags.string, tags.special(tags.string), tags.regexp], class: 'text-secondary' },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], class: 'text-code-number' },
  {
    tag: [
      tags.propertyName,
      tags.attributeName,
      tags.definition(tags.variableName),
      tags.typeName,
      tags.tagName,
    ],
    class: 'text-code-property',
  },
  { tag: tags.comment, class: 'text-code-comment italic' },
  { tag: tags.invalid, class: 'text-error' },
])

type Tree = Parameters<typeof highlightCode>[1]
interface Parser {
  parse(input: string): Tree
}
type Loader = () => Promise<Parser>

// The Lezer grammars load without CodeMirror's editor packages, so a page
// with static code alone stays light. Shell has no Lezer grammar; its legacy
// mode needs @codemirror/language, which only pages with shell code load.
const loaders: Record<Exclude<CodeLanguage, 'text'>, Loader> = {
  javascript: async () => (await import('@lezer/javascript')).parser,
  'typescript-type': async () =>
    (await import('@lezer/javascript')).parser.configure({ dialect: 'ts' }),
  json: async () => (await import('@lezer/json')).parser,
  html: async () => {
    const [html, js] = await Promise.all([import('@lezer/html'), import('@lezer/javascript')])
    return html.parser.configure({
      wrap: html.configureNesting([{ tag: 'script', parser: js.parser }]),
    })
  },
  shell: async () => (await import('./shellParser')).shellParser,
}

const loaded = new Map<CodeLanguage, Parser>()
const loading = new Map<CodeLanguage, Promise<Parser>>()

function load(language: Exclude<CodeLanguage, 'text'>): Promise<Parser> {
  let parser = loading.get(language)
  if (!parser) {
    parser = loaders[language]().then(
      (ready) => {
        loaded.set(language, ready)
        return ready
      },
      (err: unknown) => {
        // A chunk that failed (a deploy replaced it, the network dropped)
        // gets another try on the next call.
        loading.delete(language)
        throw err
      },
    )
    loading.set(language, parser)
  }
  return parser
}

/**
 * Splits `code` into colored runs when the language's parser has already
 * loaded, so a block whose code changes recolors without a plain frame.
 * Gives null when the parser hasn't loaded yet.
 */
export function highlightLoaded(code: string, language: CodeLanguage): CodeRun[] | null {
  if (language === 'text') return [{ text: code, class: '' }]
  const parser = loaded.get(language)
  if (!parser) return null
  // A bare type isn't a valid program, so it parses as the right side of an
  // alias and the alias stays out of the output.
  const prefix = language === 'typescript-type' ? 'type T = ' : ''
  const source = prefix + code
  const runs: CodeRun[] = []
  highlightCode(
    source,
    parser.parse(source),
    codeHighlighter,
    (text, classes) => runs.push({ text, class: classes }),
    () => runs.push({ text: '\n', class: '' }),
    prefix.length,
  )
  return runs
}

/** Splits `code` into colored runs. Each language's parser loads on first use. */
export async function highlight(code: string, language: CodeLanguage): Promise<CodeRun[]> {
  if (language !== 'text') await load(language)
  return highlightLoaded(code, language) ?? [{ text: code, class: '' }]
}
