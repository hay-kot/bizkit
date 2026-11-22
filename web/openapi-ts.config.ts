import { defineConfig } from '@hey-api/openapi-ts'

// Regenerate with `mise run gen`, which writes ../openapi.json first.
export default defineConfig({
  input: '../openapi.json',
  output: { path: 'src/api/gen', postProcess: ['prettier'] },
  plugins: ['@hey-api/client-fetch', '@hey-api/typescript', '@hey-api/sdk', '@pinia/colada'],
})
