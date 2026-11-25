import { StreamLanguage } from '@codemirror/language'
import { shell } from '@codemirror/legacy-modes/mode/shell'

// A static import, so the bundler can drop the parts of @codemirror/language
// the editor doesn't use; a dynamic import of the package would keep them all.
export const shellParser = StreamLanguage.define(shell).parser
