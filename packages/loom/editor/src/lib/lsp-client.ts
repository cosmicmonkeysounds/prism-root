// Browser-side LSP client. One long-lived `Workspace` from `@loom/core`
// is kept in sync with the editor's open-file map. The Loom engine is
// now pure TypeScript (no wasm), so this is fully synchronous — but the
// public functions stay async so existing callers (Outline / References)
// need no change.

import { Workspace } from '@loom/core/lsp'

const SCHEME = 'inmemory:'

export function uriFor(path: string): string {
  // Encode each path segment so unusual filenames stay valid URIs and
  // round-trip with `pathForUri` on the way back.
  return `${SCHEME}//${path.split('/').map(encodeURIComponent).join('/')}`
}

/**
 * Exact inverse of `uriFor` — decode each segment. (The lossy
 * `decodeURIComponent(uri.replace(...))` shortcut breaks if a filename
 * ever contains an encoded slash, so decode per-segment.)
 */
export function pathForUri(uri: string): string {
  const withoutScheme = uri.startsWith(`${SCHEME}//`) ? uri.slice(SCHEME.length + 2) : uri
  return withoutScheme.split('/').map(decodeURIComponent).join('/')
}

let ws: Workspace | null = null

/**
 * The singleton `Workspace`, constructed lazily and synchronously. The
 * engine has no async init, so commands / CM extensions can grab it inline.
 */
export function lspWorkspaceSync(): Workspace {
  if (!ws) ws = new Workspace()
  return ws
}

/** Async wrapper kept for legacy callers (Outline / References panels). */
export async function lspWorkspace(): Promise<Workspace> {
  return lspWorkspaceSync()
}

/**
 * The raw source text the Workspace currently holds for `uri`, or `null`.
 * Lets panels preview a line from a file that isn't an open tab (the whole
 * project is indexed, so this covers unopened files too).
 */
export function docText(uri: string): string | null {
  return lspWorkspaceSync().docs.get(uri)?.text ?? null
}

/**
 * Push the current state of every open file into the LSP workspace.
 * Best-effort: any single file failing is logged and skipped; the panel
 * still gets results for the rest.
 */
export async function syncOpenFiles(
  files: Record<string, { contents: string }>,
): Promise<void> {
  const workspace = lspWorkspaceSync()
  for (const [path, file] of Object.entries(files)) {
    try {
      workspace.open(uriFor(path), file.contents)
    } catch (err) {
      console.warn('lsp.sync: skipping', path, err)
    }
  }
}
