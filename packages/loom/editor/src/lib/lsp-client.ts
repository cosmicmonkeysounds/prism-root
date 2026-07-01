// Browser-side LSP client. One long-lived `Workspace` from `@loom/core`
// is kept in sync with the editor's open-file map. The Loom engine is
// now pure TypeScript (no wasm), so this is fully synchronous — but the
// public functions stay async so existing callers (Outline / References)
// need no change.

import { Workspace } from '@loom/core/lsp'

const SCHEME = 'inmemory:'

export function uriFor(path: string): string {
  // Encode each path segment so unusual filenames stay valid URIs and
  // round-trip with the editor's `decodeURIComponent` on the way back.
  return `${SCHEME}//${path.split('/').map(encodeURIComponent).join('/')}`
}

let ws: Workspace | null = null

export async function lspWorkspace(): Promise<Workspace> {
  if (!ws) ws = new Workspace()
  return ws
}

/**
 * Push the current state of every open file into the LSP workspace.
 * Best-effort: any single file failing is logged and skipped; the panel
 * still gets results for the rest.
 */
export async function syncOpenFiles(
  files: Record<string, { contents: string }>,
): Promise<void> {
  const workspace = await lspWorkspace()
  for (const [path, file] of Object.entries(files)) {
    try {
      workspace.open(uriFor(path), file.contents)
    } catch (err) {
      console.warn('lsp.sync: skipping', path, err)
    }
  }
}
