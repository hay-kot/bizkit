/** @type {import('prettier').Config} */
export default {
  semi: false,
  singleQuote: true,
  printWidth: 100,
  // Orders classes; the eslint class-order rule stays off so they don't fight.
  plugins: ['prettier-plugin-tailwindcss'],
  tailwindStylesheet: './src/styles/app.css',
}
