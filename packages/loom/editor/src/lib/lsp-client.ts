// Browser-side LSP client. One long-lived `LspWorkspace` is built on
// first request and kept in sync with the editor's open-file map.
// All public functions are async so a caller that arrives before the
// wasm bundle has loaded just waits for the same promise.
//
// Phase 6 follow-up of the Loom IDE redesign: Outline + References
// panels use this instead of their initial string-scan placeholders.

import type { LspWorkspace } from '@/loom-wasm/loom_wasm'

const SCHEME = 'inmemory:'

export function uriFor(path: string): string {
  // Encode each path segment so unusual filenames don't break the URL
  // parser the wasm side uses (`url::Url`).
  return `${SCHEME}//${path.split('/').map(encodeURIComponent).join('/')}`
}

let modulePromise: Promise<typeof import('@/loom-wasm/loom_wasm')> | null = null
let wsPromise: Promise<LspWorkspace> | null = null

async function loadModule(): Promise<typeof import('@/loom-wasm/loom_wasm')> {
  if (!modulePromise) {
    modulePromise = (async () => {
      const mod = await import('@/loom-wasm/loom_wasm')
      await mod.default()
      return mod
    })().catch((err) => {
      modulePromise = null
      throw err
    })
  }
  return modulePromise
}

export async function lspWorkspace(): Promise<LspWorkspace> {
  if (!wsPromise) {
    wsPromise = loadModule().then((mod) => new mod.LspWorkspace())
  }
  return wsPromise
}

/**
 * Push the current state of every open file into the LSP workspace.
 * Best-effort: any single file failing (e.g. invalid URI) is logged
 * and skipped; the panel still gets results for the rest.
 */
export async function syncOpenFiles(
  files: Record<string, { contents: string }>,
): Promise<void> {
  const ws = await lspWorkspace()
  for (const [path, file] of Object.entries(files)) {
    try {
      ws.open(uriFor(path), file.contents)
    } catch (err) {
      console.warn('lsp.sync: skipping', path, err)
    }
  }
}
