// Whole-project LSP indexing.
//
// The singleton `Workspace` only knows about documents it has been handed.
// Historically that was just the open tabs (`syncOpenFiles`), so cross-file
// go-to-definition / find-references / project diagnostics silently missed
// anything in an unopened `.loom` file. This module walks the workspace tree
// and pushes EVERY `.loom` file into the Workspace, keeping it diffed so a
// keystroke only re-parses the one edited buffer.
//
// Reads are backend-aware: server-backed FsEntries carry their `content`
// inline (free); local (File System Access) entries are read lazily through
// the handle and gated on `File.lastModified` so an unchanged file is never
// re-read.

import { create } from 'zustand'
import type { FsEntry } from '@/lib/fs'
import { lspWorkspaceSync, uriFor } from '@/lib/lsp-client'
import { useWorkspace } from '@/store/workspace'

/** uri -> the exact text last handed to the Workspace (skip no-op reparses). */
const indexedText = new Map<string, string>()
/** uri -> local file `lastModified`, so unchanged files aren't re-read. */
const indexedMtime = new Map<string, number>()

/**
 * A monotonically-increasing "the Workspace's contents changed" signal.
 * React views (References / CommandPalette symbols) subscribe to `gen` so
 * they recompute when async indexing / buffer syncs land, instead of relying
 * on the tree object identity (which doesn't change when text does).
 */
export const useLspIndexGen = create<{ gen: number; bump: () => void }>((set) => ({
  gen: 0,
  bump: () => set((s) => ({ gen: s.gen + 1 })),
}))
const bump = () => useLspIndexGen.getState().bump()

function collectLoom(entry: FsEntry, out: FsEntry[]): void {
  if (entry.kind === 'file') {
    if (entry.path.endsWith('.loom')) out.push(entry)
    return
  }
  for (const c of entry.children ?? []) collectLoom(c, out)
}

/**
 * Full (re)index of the project tree. Diffed: unchanged docs are skipped,
 * vanished docs are closed. A live unsaved buffer always wins over the
 * on-disk / inline copy — and is read from the LIVE store at push time (not a
 * captured snapshot), so a walk that races an edit never regresses the buffer.
 *
 * `shouldCancel` lets a superseded walk (newer tree) bail before mutating the
 * Workspace against a stale tree.
 */
export async function indexProjectTree(
  root: FsEntry,
  shouldCancel: () => boolean = () => false,
): Promise<void> {
  const ws = lspWorkspaceSync()
  const files: FsEntry[] = []
  collectLoom(root, files)

  const seen = new Set<string>()
  const batch: Array<[string, string]> = []

  for (const f of files) {
    if (shouldCancel()) return
    const uri = uriFor(f.path)
    seen.add(uri)

    // Read the live buffer at push time so an edit during our awaits wins.
    const open = useWorkspace.getState().openFiles[f.path]
    if (open) {
      if (indexedText.get(uri) !== open.contents) {
        batch.push([uri, open.contents])
        // An open buffer's text is not the disk's — force a disk re-read if the
        // tab is later closed (discarding edits) so we don't keep stale text.
        indexedMtime.delete(uri)
      }
      continue
    }
    if (f.backend === 'server') {
      const text = f.content ?? ''
      if (indexedText.get(uri) !== text) batch.push([uri, text])
      continue
    }
    if (f.handle) {
      // Local: lazy read, mtime-gated.
      try {
        const fileObj = await (f.handle as FileSystemFileHandle).getFile()
        if (indexedMtime.get(uri) === fileObj.lastModified && indexedText.has(uri)) continue
        indexedMtime.set(uri, fileObj.lastModified)
        const text = await fileObj.text()
        if (indexedText.get(uri) !== text) batch.push([uri, text])
      } catch {
        // Deleted mid-walk; the prune pass below drops it.
      }
    }
  }

  if (shouldCancel()) return

  let changed = false
  if (batch.length > 0) {
    ws.updateMany(batch)
    for (const [uri, text] of batch) indexedText.set(uri, text)
    changed = true
  }

  // Prune docs whose file vanished from the tree.
  for (const uri of [...indexedText.keys()]) {
    if (!seen.has(uri)) {
      ws.close(uri)
      indexedText.delete(uri)
      indexedMtime.delete(uri)
      changed = true
    }
  }

  if (changed) bump()
}

/** Keep the in-progress (unsaved) buffer of one file live in the Workspace. */
export function syncBuffer(path: string, contents: string): void {
  const uri = uriFor(path)
  if (indexedText.get(uri) === contents) return
  lspWorkspaceSync().update(uri, contents)
  indexedText.set(uri, contents)
  // Buffer text != disk text: force a later disk-based reindex to re-read.
  indexedMtime.delete(uri)
  bump()
}

/** Drop a path from the Workspace + the diff cache (delete / rename). */
export function dropIndexedPath(path: string): void {
  const uri = uriFor(path)
  if (!indexedText.has(uri)) return
  lspWorkspaceSync().close(uri)
  indexedText.delete(uri)
  indexedMtime.delete(uri)
  bump()
}

/** Reset the Workspace + diff cache (project switch / close). */
export function resetIndexCache(): void {
  lspWorkspaceSync().reset()
  indexedText.clear()
  indexedMtime.clear()
  bump()
}
