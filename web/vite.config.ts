import { fileURLToPath, URL } from 'node:url'

import tailwindcss from '@tailwindcss/vite'
import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vitest/config'

// svx sets BIZKIT_API_URL to the stack's API; plain `mise run web:dev` uses
// the default port.
const api = process.env.BIZKIT_API_URL ?? 'http://127.0.0.1:8484'

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  server: {
    proxy: {
      '/api': { target: api, changeOrigin: false },
      '/mcp': { target: api, changeOrigin: false },
      '/m/': { target: api, changeOrigin: false },
    },
  },
  build: { outDir: 'dist', emptyOutDir: true },
  test: { environment: 'happy-dom', include: ['src/**/*.test.ts'] },
})
