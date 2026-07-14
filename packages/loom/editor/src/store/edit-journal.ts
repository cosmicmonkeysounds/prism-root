// The story edit journal — cross-surface undo/redo for structural
// edits that don't originate in the text editor's own keyboard: canvas
// connect/rewire/create/rename/delete, word-block and BeatStrip edits,
// tray field writes, and the context-menu beat rename. Each entry is
// an atomic multi-file batch of `{path, before, after}` snapshots with
// a human label (shown in the command palette).
//
// ⌘Z routing (see `StudioShell`): focus inside the CodeMirror editor →
// CM's own history; anywhere else → this journal. The two histories
// can describe the same buffer, so application is guarded: an entry
// only applies while every file still holds the text the entry expects
// — if a buffer moved on (typed edits, a CM undo of the same change),
// the stale entry drops instead of clobbering the newer text.

import { create } from 'zustand'
import { docText, uriFor } from '@/lib/lsp-client'
import { syncBuffer } from '@/lib/lsp-index'
import { findFileEntryByPath } from '@/lib/lsp-nav'
import { useWorkspace } from '@/store/workspace'

export type JournalEdit = { path: string; before: string; after: string }
export type JournalEntry = { label: string; edits: JournalEdit[]; at: number }

const MAX_ENTRIES = 100

/** The text a path currently holds (open buffer first, LSP index else). */
function currentText(path: string): string | null {
  const open = useWorkspace.getState().openFiles[path]
  if (open) return open.contents
  return docText(uriFor(path))
}

/** Write one file back through the workspace (opening it as a tab). */
async function restore(path: string, text: string): Promise<boolean> {
  const ws = useWorkspace.getState()
  if (!ws.openFiles[path]) {
    const entry = ws.root ? findFileEntryByPath(ws.root, path) : null
    if (!entry) return false
    await ws.openFile(entry)
  }
  useWorkspace.getState().updateContents(path, text)
  syncBuffer(path, text)
  return true
}

/**
 * Apply an entry in one direction. Atomic: verifies every file still
 * holds the expected text before touching any of them.
 */
async function applyEntry(entry: JournalEntry, dir: 'undo' | 'redo'): Promise<boolean> {
  for (const ed of entry.edits) {
    if (currentText(ed.path) !== (dir === 'undo' ? ed.after : ed.before)) return false
  }
  for (const ed of entry.edits) {
    if (!(await restore(ed.path, dir === 'undo' ? ed.before : ed.after))) return false
  }
  return true
}

type EditJournalState = {
  undoStack: JournalEntry[]
  redoStack: JournalEntry[]
  /** Transient outcome line ("Undid Connect a → b", "Can't undo — …"). */
  notice: { text: string; at: number } | null

  record(label: string, edits: JournalEdit[]): void
  /** Undo the newest entry. Resolves to the entry label, or null. */
  undo(): Promise<string | null>
  redo(): Promise<string | null>
  clear(): void
}

export const useEditJournal = create<EditJournalState>((set, get) => ({
  undoStack: [],
  redoStack: [],
  notice: null,

  record: (label, edits) => {
    const real = edits.filter((e) => e.before !== e.after)
    if (real.length === 0) return
    set((s) => ({
      undoStack: [...s.undoStack, { label, edits: real, at: Date.now() }].slice(-MAX_ENTRIES),
      redoStack: [],
    }))
  },

  undo: async () => {
    const s = get()
    const entry = s.undoStack[s.undoStack.length - 1]
    if (entry === undefined) {
      set({ notice: { text: 'Nothing to undo.', at: Date.now() } })
      return null
    }
    // The entry leaves the stack either way — applied, or stale (the
    // buffer moved on; clobbering newer text would be worse than
    // forgetting one hop).
    const ok = await applyEntry(entry, 'undo')
    set((st) => ({
      undoStack: st.undoStack.slice(0, -1),
      redoStack: ok ? [...st.redoStack, entry] : st.redoStack,
      notice: {
        text: ok ? `Undid: ${entry.label}` : `Can't undo “${entry.label}” — the file changed since.`,
        at: Date.now(),
      },
    }))
    return ok ? entry.label : null
  },

  redo: async () => {
    const s = get()
    const entry = s.redoStack[s.redoStack.length - 1]
    if (entry === undefined) {
      set({ notice: { text: 'Nothing to redo.', at: Date.now() } })
      return null
    }
    const ok = await applyEntry(entry, 'redo')
    set((st) => ({
      redoStack: st.redoStack.slice(0, -1),
      undoStack: ok ? [...st.undoStack, entry] : st.undoStack,
      notice: {
        text: ok ? `Redid: ${entry.label}` : `Can't redo “${entry.label}” — the file changed since.`,
        at: Date.now(),
      },
    }))
    return ok ? entry.label : null
  },

  clear: () => set({ undoStack: [], redoStack: [], notice: null }),
}))

// A new workspace is a new history — clear when the project identity
// changes. (Keyed by project id / root NAME, not the root object:
// `refreshTree` swaps the root object on every file-system event.)
let lastProjectKey: string | null = null
useWorkspace.subscribe((s) => {
  const key = s.projectId ?? s.root?.name ?? null
  if (key !== lastProjectKey) {
    lastProjectKey = key
    useEditJournal.getState().clear()
  }
})
