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

const loaders: Record<Exclude<CodeLanguage, 'text'>, Loader> = {
  javascript: async () => (await import('@codemirror/lang-javascript')).javascriptLanguage.parser,
  'typescript-type': async () =>
    (await import('@codemirror/lang-javascript')).typescriptLanguage.parser,
  json: async () => (await import('@codemirror/lang-json')).jsonLanguage.parser,
  html: async () => (await import('@codemirror/lang-html')).htmlLanguage.parser,
  shell: async () => {
    const [{ StreamLanguage }, { shell }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/shell'),
    ])
    return StreamLanguage.define(shell).parser
  },
}

const parsers = new Map<CodeLanguage, Promise<Parser>>()

function parserFor(language: Exclude<CodeLanguage, 'text'>): Promise<Parser> {
  let parser = parsers.get(language)
  if (!parser) {
    parser = loaders[language]()
    parsers.set(language, parser)
  }
  return parser
}

/** Splits `code` into colored runs. Each language's parser loads on first use. */
export async function highlight(code: string, language: CodeLanguage): Promise<CodeRun[]> {
  if (language === 'text') return [{ text: code, class: '' }]
  const parser = await parserFor(language)
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
