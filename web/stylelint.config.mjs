/** Colors live in src/styles/theme.css only; everything else uses DaisyUI names. */
const tailwindAtRules = ['plugin', 'theme', 'source', 'custom-variant', 'utility', 'variant', 'apply', 'reference']
const colorFunctions = ['rgb', 'rgba', 'hsl', 'hsla', 'hwb', 'lab', 'lch', 'oklab', 'oklch', 'color']

export default {
  extends: ['stylelint-config-standard', 'stylelint-config-recommended-vue'],
  rules: {
    'at-rule-no-unknown': [true, { ignoreAtRules: tailwindAtRules }],
    'at-rule-no-deprecated': [true, { ignoreAtRules: ['apply'] }],
    'import-notation': 'string',
    'color-no-hex': true,
    'color-named': 'never',
    'function-disallowed-list': colorFunctions,
  },
  overrides: [
    {
      files: ['src/styles/theme.css'],
      rules: {
        'function-disallowed-list': null,
        'custom-property-pattern': null,
        'declaration-property-value-no-unknown': null,
        'property-no-unknown': null,
      },
    },
  ],
}
