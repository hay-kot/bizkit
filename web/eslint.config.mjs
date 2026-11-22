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
      parserOptions: { parser: tseslint.parser, extraFileExtensions: ['.vue'], sourceType: 'module' },
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
      'better-tailwindcss/no-restricted-classes': [
        'error',
        {
          restrict: [
            {
              pattern: '^.*\\[.*$',
              message: 'Arbitrary values $0 bypass the design tokens: use a Tailwind or DaisyUI class.',
            },
            {
              pattern: `^(?:.*:)?(?:${colorUtilities})-(?:(?:${palette})-\\d+|black|white)(?:/\\d+)?$`,
              message: 'Raw palette color $0: use DaisyUI semantic colors (primary, base-200, error, ...).',
            },
          ],
        },
      ],
      // prettier-plugin-tailwindcss orders classes.
      'better-tailwindcss/enforce-consistent-class-order': 'off',

      'vue/no-static-inline-styles': 'error',
      'vue/no-restricted-v-bind': ['error', { argument: 'style', message: 'Use classes, not inline styles.' }],
      '@eslint-community/eslint-comments/require-description': ['error', { ignore: [] }],
      'vue/multi-word-component-names': 'off',
    },
  },
  {
    files: ['src/**/*.{ts,vue}'],
    ignores: ['src/queries/**', 'src/api/client.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['@/api/gen', '@/api/gen/*', '**/api/gen', '**/api/gen/*'],
              message: 'Only src/queries and src/api/client.ts import the generated client.',
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
