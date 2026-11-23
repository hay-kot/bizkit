<script setup lang="ts">
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { javascript } from '@codemirror/lang-javascript'
import { json } from '@codemirror/lang-json'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { EditorState } from '@codemirror/state'
import { EditorView, keymap, lineNumbers } from '@codemirror/view'
import { tags } from '@lezer/highlight'
import { onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue'

const props = defineProps<{
  label: string
  language: 'javascript' | 'json'
  readonly: boolean
  minLines: number
  maxLines: number
}>()
const model = defineModel<string>({ required: true })
const host = useTemplateRef<HTMLDivElement>('host')
let view: EditorView | undefined

// DaisyUI exposes its theme as CSS variables, so the editor follows the
// light and dark themes without colors of its own.
const highlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.operatorKeyword, tags.modifier], color: 'var(--color-primary)' },
  { tag: [tags.string, tags.special(tags.string), tags.regexp], color: 'var(--color-secondary)' },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: 'var(--color-accent)' },
  { tag: [tags.propertyName, tags.definition(tags.variableName)], color: 'var(--color-info)' },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment],
    color: 'color-mix(in oklab, var(--color-base-content) 55%, transparent)',
    fontStyle: 'italic',
  },
  { tag: tags.invalid, color: 'var(--color-error)' },
])

const LINE_HEIGHT_EM = 1.5

function theme() {
  return EditorView.theme({
    '&': {
      backgroundColor: 'var(--color-base-100)',
      color: 'var(--color-base-content)',
      border: 'var(--border) solid var(--color-base-300)',
      borderRadius: 'var(--radius-field)',
    },
    '&.cm-focused': {
      outline: '2px solid var(--color-primary)',
      outlineOffset: '2px',
    },
    '.cm-scroller': {
      fontFamily: 'inherit',
      lineHeight: String(LINE_HEIGHT_EM),
      minHeight: `calc(${props.minLines * LINE_HEIGHT_EM}em + 8px)`,
      maxHeight: `calc(${props.maxLines * LINE_HEIGHT_EM}em + 8px)`,
      overflow: 'auto',
    },
    '.cm-content': { caretColor: 'var(--color-base-content)', padding: '4px 0' },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--color-base-content)' },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: 'color-mix(in oklab, var(--color-primary) 25%, transparent)',
    },
    '.cm-gutters': {
      backgroundColor: 'var(--color-base-200)',
      color: 'color-mix(in oklab, var(--color-base-content) 50%, transparent)',
      borderRight: 'var(--border) solid var(--color-base-300)',
    },
    '.cm-activeLine': { backgroundColor: 'transparent' },
  })
}

onMounted(() => {
  if (!host.value) return
  view = new EditorView({
    parent: host.value,
    state: EditorState.create({
      doc: model.value,
      extensions: [
        lineNumbers(),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        props.language === 'json' ? json() : javascript(),
        syntaxHighlighting(highlight),
        theme(),
        EditorState.readOnly.of(props.readonly),
        EditorView.editable.of(!props.readonly),
        EditorView.contentAttributes.of({
          'aria-label': props.label,
          spellcheck: 'false',
          autocapitalize: 'off',
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) model.value = update.state.doc.toString()
        }),
      ],
    }),
  })
})

watch(model, (value) => {
  if (!view || view.state.doc.toString() === value) return
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } })
})

onBeforeUnmount(() => view?.destroy())
</script>

<template>
  <div ref="host" class="font-mono text-xs"></div>
</template>
