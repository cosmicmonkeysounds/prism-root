// Owns the LSP project-index lifecycle for an open workspace. Mounted once
// by the Studio shell; renders nothing.
//
// Two effects:
//   1. On any structural tree change (`root` identity), re-walk + (re)index
//      every `.loom` file — debounced so an FS-observer burst coalesces.
//   2. On every active-buffer edit, keep that one doc live in the Workspace
//      (debounced) so hover / goto / completion / diagnostics reflect what
//      the user is currently typing before they save.
//
// The store replaces `root` with a fresh object on openRoot / openServerProject
// / refreshTree / restoreRoot, so subscribing to `root` identity is the single
// sufficient reindex trigger; keystrokes never re-walk the tree.

import { useEffect } from 'react'
import { useWorkspace } from '@/store/workspace'
import { useSettings } from '@/store/settings'
import { indexProjectTree, syncBuffer } from '@/lib/lsp-index'

export function useLspProjectIndex(): void {
  const root = useWorkspace((s) => s.root)
  const activePath = useWorkspace((s) => s.activePath)
  const activeContents = useWorkspace((s) =>
    s.activePath ? s.openFiles[s.activePath]?.contents : undefined,
  )
  const indexWholeProject = useSettings((s) => s.indexWholeProject)

  useEffect(() => {
    if (!root || !indexWholeProject) return
    let cancelled = false
    const t = setTimeout(() => {
      if (cancelled) return
      // Pass a live cancel check: a newer tree (this effect re-running)
      // flips `cancelled`, so a superseded walk bails before mutating.
      void indexProjectTree(root, () => cancelled)
    }, 150)
    return () => {
      cancelled = true
      clearTimeout(t)
    }
  }, [root, indexWholeProject])

  // Keep the in-progress active buffer live even when whole-project indexing
  // is off, so hover / goto / diagnostics at least cover the current file.
  useEffect(() => {
    if (!activePath || activeContents === undefined) return
    const t = setTimeout(() => syncBuffer(activePath, activeContents), 120)
    return () => clearTimeout(t)
  }, [activePath, activeContents])
}
