import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'node:path'

export default defineConfig({
  plugins: [react(), tailwindcss()],
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
