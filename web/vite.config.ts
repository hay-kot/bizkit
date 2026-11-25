import { createReadStream, readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, URL } from 'node:url'

import tailwindcss from '@tailwindcss/vite'
import vue from '@vitejs/plugin-vue'
import type { Plugin } from 'vite'
import { defineConfig } from 'vitest/config'

// svx sets BIZKIT_API_URL to the stack's API; plain `mise run web:dev` uses
// the default port.
const api = process.env.BIZKIT_API_URL ?? 'http://127.0.0.1:8484'

const pdfjsRoot = fileURLToPath(new URL('./node_modules/pdfjs-dist', import.meta.url))
const pdfjsVersion: string = JSON.parse(
  readFileSync(join(pdfjsRoot, 'package.json'), 'utf8'),
).version

/**
 * The pdf.js data files bizkit serves, each checked against the license allow
 * list: the Adobe CMaps (BSD-3-Clause) and the image and color decoders
 * (JBIG2: BSD-3-Clause and Apache-2.0; OpenJPEG: BSD-2-Clause; qcms: MIT),
 * with their license files. Left out: `standard_fonts/`, whose Liberation
 * fonts are SIL OFL 1.1, and the QuickJS build, which only runs document
 * scripts, which bizkit never enables.
 */
function pdfjsFiles(): string[] {
  const cmaps = readdirSync(join(pdfjsRoot, 'cmaps')).map((name) => `cmaps/${name}`)
  const wasm = readdirSync(join(pdfjsRoot, 'wasm'))
    .filter((name) =>
      /^(?:jbig2|openjpeg|qcms)|^LICENSE_(?:PDFJS_)?(?:JBIG2|OPENJPEG|QCMS)$/.test(name),
    )
    .map((name) => `wasm/${name}`)
  return [...cmaps, ...wasm]
}

/**
 * Serves the pdf.js data files at `/pdfjs/<version>/` in dev and writes them
 * there in the build. The version in the path lets bizkit-web cache them
 * like hashed assets.
 */
function pdfjsAssets(): Plugin {
  const base = `pdfjs/${pdfjsVersion}/`
  return {
    name: 'bizkit-pdfjs-assets',
    configureServer(server) {
      const files = new Set(pdfjsFiles())
      server.middlewares.use(`/${base}`, (req, res, next) => {
        const path = decodeURIComponent((req.url ?? '').split('?')[0]!.replace(/^\//, ''))
        if (!files.has(path)) return next()
        const file = join(pdfjsRoot, path)
        res.setHeader('Content-Length', statSync(file).size)
        if (path.endsWith('.wasm')) res.setHeader('Content-Type', 'application/wasm')
        createReadStream(file).pipe(res)
      })
    },
    generateBundle() {
      for (const path of pdfjsFiles()) {
        this.emitFile({
          type: 'asset',
          fileName: `${base}${path}`,
          source: readFileSync(join(pdfjsRoot, path)),
        })
      }
    },
  }
}

export default defineConfig({
  plugins: [vue(), tailwindcss(), pdfjsAssets()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  server: {
    proxy: {
      '/api': { target: api, changeOrigin: false, xfwd: true },
      '/mcp': { target: api, changeOrigin: false, xfwd: true },
      '/m/': { target: api, changeOrigin: false, xfwd: true },
    },
  },
  build: { outDir: 'dist', emptyOutDir: true },
  test: { environment: 'happy-dom', include: ['src/**/*.test.ts'] },
})
