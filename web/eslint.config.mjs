import comments from '@eslint-community/eslint-plugin-eslint-comments/configs'
import js from '@eslint/js'
import prettier from 'eslint-config-prettier'
import betterTailwind from 'eslint-plugin-better-tailwindcss'
import vue from 'eslint-plugin-vue'
import globals from 'globals'
import tseslint from 'typescript-eslint'

const palette =
  'slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose'
const colorUtilities =
  'bg|text|border|ring|outline|fill|stroke|from|via|to|divide|shadow|accent|caret|decoration|placeholder'

// Everywhere: only design tokens.
const tokenRestrictions = [
  {
    pattern: '^.*\\[.*$',
    message: 'Arbitrary values $0 bypass the design tokens: use a Tailwind or DaisyUI class.',
  },
  {
    pattern: `^(?:.*:)?(?:${colorUtilities})-(?:(?:${palette})-\\d+|black|white)(?:/\\d+)?$`,
    message: 'Raw palette color $0: use DaisyUI semantic colors (primary, base-200, error, ...).',
  },
]

// Outside src/components/ui: DaisyUI's interactive controls are wrapped by
// our primitives, which carry the keyboard handling and the look.
const controlRestrictions = [
  {
    pattern: '^(?:checkbox|toggle|radio|range|file-input)(?:-.+)?$',
    message: '$0: use components/ui/Checkbox.vue.',
  },
  {
    pattern:
      '^select(?:-(?:xs|sm|md|lg|xl|ghost|bordered|primary|secondary|accent|neutral|info|success|warning|error))?$',
    message: '$0: use components/ui/Select.vue.',
  },
  { pattern: '^table(?:-.+)?$', message: '$0: use components/ui/DataTable.vue.' },
  {
    pattern: '^(?:modal|drawer)(?:-.+)?$',
    message: '$0: use components/ui/FormPanel.vue or Overlay.vue.',
  },
  { pattern: '^dropdown(?:-.+)?$', message: '$0: use components/ui/Menu.vue.' },
  {
    pattern: '^loading(?:-.+)?$',
    message: '$0: use SubmitButton, Spinner or SkeletonRows from components/ui.',
  },
]

// Outside src/components/ui: a row's name is never a hover-underlined link,
// because only its text would be the target. Inline links in prose keep `link`.
const rowLinkRestrictions = [
  {
    pattern: '^link-hover$',
    message:
      '$0: for a row that opens a page or panel, use DataTable rowTo or @row-click, so the whole row is the target. Inline text links use `link`.',
  },
]

// In src/pages: one column, width set on <Page>.
const pageRestrictions = [
  {
    pattern: '^(?:.*:)?(?:(?:grid-cols|col-span|columns|max-w)-.+|grid-flow-col(?:-dense)?)$',
    message:
      '$0: pages are one column inside <Page>; set the width on Page and put grids in a component.',
  },
]

const generatedClient = {
  group: ['@/api/gen', '@/api/gen/*', '**/api/gen', '**/api/gen/*'],
  message: 'Only src/queries and src/api/client.ts import the generated client.',
}

// Dates go through src/lib/format.ts; native dialogs never appear.
const syntaxRestrictions = [
  {
    selector: 'CallExpression[callee.property.name=/^toLocale(?:Date|Time)?String$/]',
    message: 'Format dates with src/lib/format.ts.',
  },
  {
    selector: "NewExpression[callee.object.name='Intl']",
    message: 'Format dates with src/lib/format.ts.',
  },
  {
    selector: 'CallExpression[callee.name=/^(?:alert|confirm|prompt)$/]',
    message: 'Use runAction() with confirm, or toast(), instead of native dialogs.',
  },
]

export default tseslint.config(
  { ignores: ['dist', 'src/api/gen', 'node_modules'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...vue.configs['flat/recommended'],
  comments.recommended,
  {
    files: ['**/*.{ts,vue}'],
    languageOptions: {
      globals: globals.browser,
      parserOptions: {
        parser: tseslint.parser,
        extraFileExtensions: ['.vue'],
        sourceType: 'module',
      },
    },
  },
  {
    files: ['**/*.{ts,vue}'],
    plugins: { 'better-tailwindcss': betterTailwind },
    settings: { 'better-tailwindcss': { entryPoint: 'src/styles/app.css' } },
    rules: {
      // Only Tailwind and DaisyUI classes. eslint-plugin-better-tailwindcss
      // 4.11.0 misses these DaisyUI 5 classes, which DaisyUI defines only in
      // compound selectors.
      'better-tailwindcss/no-unknown-classes': [
        'error',
        { ignore: ['^tab-active$', '^dropdown-content$', '^dropdown-hover$', '^modal-open$'] },
      ],
      'better-tailwindcss/no-conflicting-classes': 'error',
      'better-tailwindcss/no-duplicate-classes': 'error',
      'better-tailwindcss/no-restricted-classes': ['error', { restrict: tokenRestrictions }],
      // prettier-plugin-tailwindcss orders classes.
      'better-tailwindcss/enforce-consistent-class-order': 'off',

      'vue/no-static-inline-styles': 'error',
      'vue/no-restricted-v-bind': [
        'error',
        { argument: 'style', message: 'Use classes, not inline styles.' },
      ],
      '@eslint-community/eslint-comments/require-description': ['error', { ignore: [] }],
      'vue/multi-word-component-names': 'off',
    },
  },
  {
    files: ['src/**/*.{ts,vue}'],
    ignores: ['src/lib/format.ts'],
    rules: {
      'no-restricted-syntax': ['error', ...syntaxRestrictions],
      'vue/no-restricted-syntax': ['error', ...syntaxRestrictions],
    },
  },
  {
    // The primitives in components/ui wrap these; everything else uses them.
    files: ['src/**/*.vue'],
    ignores: ['src/components/ui/**'],
    rules: {
      'better-tailwindcss/no-restricted-classes': [
        'error',
        { restrict: [...tokenRestrictions, ...controlRestrictions, ...rowLinkRestrictions] },
      ],
      'vue/no-restricted-html-elements': [
        'error',
        { element: ['table'], message: 'Use components/ui/DataTable.vue.' },
        { element: ['select'], message: 'Use components/ui/Select.vue.' },
        { element: ['dialog'], message: 'Use components/ui/FormPanel.vue or Overlay.vue.' },
      ],
      'vue/no-restricted-static-attribute': [
        'error',
        {
          key: 'type',
          value: 'checkbox',
          element: 'input',
          message: 'Use components/ui/Checkbox.vue.',
        },
        { key: 'type', value: 'radio', element: 'input', message: 'Use components/ui/Select.vue.' },
      ],
    },
  },
  {
    // See web/DESIGN.md. Repeats the shared lists because a later block's
    // no-restricted-classes replaces the earlier one.
    files: ['src/pages/**/*.vue'],
    rules: {
      'better-tailwindcss/no-restricted-classes': [
        'error',
        {
          restrict: [
            ...tokenRestrictions,
            ...controlRestrictions,
            ...rowLinkRestrictions,
            ...pageRestrictions,
          ],
        },
      ],
    },
  },
  {
    files: ['src/**/*.{ts,vue}'],
    ignores: ['src/queries/**', 'src/api/client.ts'],
    rules: { 'no-restricted-imports': ['error', { patterns: [generatedClient] }] },
  },
  {
    // Pages and layouts compose; data and overlays come through one door each.
    files: ['src/pages/**/*.{ts,vue}', 'src/layouts/**/*.{ts,vue}'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          paths: [
            { name: '@pinia/colada', message: 'Use the composables in src/queries.' },
            {
              name: '@/components/ui/Overlay.vue',
              message: 'Use FormPanel.vue, or wrap the overlay in a component.',
            },
            {
              name: '@/lib/confirm',
              message: 'Use runAction({ confirm }) from src/lib/action.ts.',
            },
          ],
          patterns: [
            generatedClient,
            {
              group: ['**/ui/ActionsMenu', '**/ui/ActionsMenu.vue'],
              message: 'Use DetailActions.vue, so header actions share one size and style.',
            },
          ],
        },
      ],
    },
  },
  {
    files: ['*.config.{ts,mjs}', '.prettierrc.mjs'],
    languageOptions: { globals: globals.node },
  },
  prettier,
)
