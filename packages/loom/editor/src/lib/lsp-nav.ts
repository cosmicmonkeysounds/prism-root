// Position mapping + cross-file navigation between the LSP (`@loom/core/lsp`)
// and CodeMirror / the workspace store. Shared by the CM extensions
// (`loom-lsp.ts`) and the References panel.

import type { EditorState } from '@codemirror/state'
import type { Location, Position, GotoDefinitionResponse } from '@loom/core/lsp'
import type { FsEntry } from '@/lib/fs'
import { pathForUri } from '@/lib/lsp-client'
import { useWorkspace } from '@/store/workspace'

/** CM absolute offset → LSP `{ line, character }` (both zero-based, UTF-16). */
export function offsetToLsp(state: EditorState, offset: number): Position {
  const line = state.doc.lineAt(offset)
  return { line: line.number - 1, character: offset - line.from }
}

/** LSP `{ line, character }` → CM absolute offset. Clamped so a stale position can't throw. */
export function lspToOffset(state: EditorState, pos: Position): number {
  const lineNo = Math.min(Math.max(pos.line + 1, 1), state.doc.lines)
  const line = state.doc.line(lineNo)
  return line.from + Math.min(Math.max(pos.character, 0), line.length)
}

/** `definitionAt` returns `Location | Location[] | null` — take the first. */
export function firstLocation(resp: GotoDefinitionResponse | null): Location | null {
  if (!resp) return null
  return Array.isArray(resp) ? (resp[0] ?? null) : resp
}

/** Depth-first search for the file `FsEntry` with an exact path. */
export function findFileEntryByPath(entry: FsEntry, path: string): FsEntry | null {
  if (entry.kind === 'file') return entry.path === path ? entry : null
  for (const c of entry.children ?? []) {
    const hit = findFileEntryByPath(c, path)
    if (hit) return hit
  }
  return null
}

/**
 * Reveal an LSP `Location` (0-based) in the active file or a sibling file.
 * Same-file uses `revealActive` (no reopen); a different file is opened via
 * `revealAt`. Store reveal APIs are 1-based, hence the `+ 1`.
 */
export async function navigateToLocation(loc: Location): Promise<void> {
  const { activePath, root, revealActive, revealAt } = useWorkspace.getState()
  const path = pathForUri(loc.uri)
  const line = loc.range.start.line + 1
  const column = loc.range.start.character + 1
  if (path === activePath) {
    revealActive(line, column)
    return
  }
  const entry = root ? findFileEntryByPath(root, path) : null
  if (entry) await revealAt(entry, line, column)
  else console.warn('navigateToLocation: no file entry for', path)
}
