import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'node:path'

// The Loom SaaS backend (auth + projects + events + live event routes).
// Override with LOOM_SERVER, e.g. http://192.168.x.x:7000.
const apiTarget = process.env.LOOM_SERVER ?? 'http://localhost:7000'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      // Same-origin from the browser so the BetterAuth session cookie flows.
      '/api': { target: apiTarget, changeOrigin: true },
      '/e': { target: apiTarget, changeOrigin: true },
      '/console': { target: apiTarget, changeOrigin: true },
    },
  },
  resolve: {
    alias: {
      // Loom engine as TypeScript source (no wasm). Subpaths first so
      // they win over the bare-package fallback.
      '@loom/core/parser': path.resolve(__dirname, '../core/src/parser/index.ts'),
      '@loom/core/lsp': path.resolve(__dirname, '../core/src/lsp/index.ts'),
      '@loom/core': path.resolve(__dirname, '../core/src/index.ts'),
      '@': path.resolve(__dirname, './src'),
    },
  },
})
