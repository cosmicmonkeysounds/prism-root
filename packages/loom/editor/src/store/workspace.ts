import { create } from 'zustand'
import type { Node, Edge } from '@xyflow/react'
import {
  createDirectory,
  createFile,
  createFsObserver,
  queryHandlePermission,
  readDirectoryTree,
  readFileText,
  removeEntry,
  renameHandle,
  requestHandlePermission,
  writeFileText,
  type FsChangeRecord,
  type FsEntry,
} from '@/lib/fs'
import { idbDel, idbGet, idbSet } from '@/lib/idb'
import { useSettings } from '@/store/settings'

function applyFormat(text: string): string {
  // Trim trailing whitespace on every line, and ensure exactly one final newline.
  const lines = text.split('\n').map((l) => l.replace(/[ \t]+$/g, ''))
  let trimmed = lines.join('\n')
  trimmed = trimmed.replace(/\n+$/, '')
  return trimmed + '\n'
}

export type OpenFile = {
  path: string
  handle: FileSystemFileHandle
  contents: string
  dirty: boolean
}

export type RootStatus = 'idle' | 'connected' | 'needs-permission' | 'denied'

export type CursorInfo = { line: number; column: number; selection: number }

const ROOT_HANDLE_KEY = 'root-handle'

type WorkspaceState = {
  root: FsEntry | null
  rootStatus: RootStatus
  openFiles: Record<string, OpenFile>
  tabOrder: string[]
  activePath: string | null
  recentlyClosed: { path: string; handle: FileSystemFileHandle }[]
  cursor: CursorInfo | null
  pendingCursor: { path: string; line: number; column: number; token: number } | null

  nodes: Node[]
  edges: Edge[]

  openRoot: (handle: FileSystemDirectoryHandle) => Promise<void>
  restoreRoot: () => Promise<void>
  requestPermission: () => Promise<void>
  closeRoot: () => Promise<void>
  refreshTree: () => Promise<void>
  openFile: (entry: FsEntry) => Promise<void>
  closeFile: (path: string) => void
  closeOthers: (path: string) => void
  closeAll: () => void
  reopenClosed: () => Promise<void>
  setActive: (path: string) => void
  cycleTab: (delta: number) => void
  activateTabByIndex: (index: number) => void
  updateContents: (path: string, contents: string) => void
  setCursor: (cursor: CursorInfo | null) => void
  revealAt: (entry: FsEntry, line: number, column?: number) => Promise<void>
  /** Reveal a 1-based line in the already-open active file (no reopen). */
  revealActive: (line: number, column?: number) => void
  saveActive: () => Promise<void>
  saveAll: () => Promise<void>
  createNewFile: (name: string) => Promise<void>
  createFileIn: (dir: FsEntry, name: string) => Promise<void>
  createDirectoryIn: (dir: FsEntry, name: string) => Promise<void>
  deleteEntry: (parent: FsEntry, entry: FsEntry) => Promise<void>
  renameEntry: (parent: FsEntry, entry: FsEntry, newName: string) => Promise<void>

  setNodes: (updater: Node[] | ((prev: Node[]) => Node[])) => void
  setEdges: (updater: Edge[] | ((prev: Edge[]) => Edge[])) => void
}

const nodeStyle = {
  background: '#18181b',
  color: '#e6e6e6',
  border: '1px solid rgba(255,255,255,0.1)',
  borderRadius: 8,
  fontSize: 12,
  padding: 8,
  minWidth: 140,
} as const

function buildNodeForPath(path: string, index: number): Node {
  return {
    id: path,
    position: { x: 60 + (index % 3) * 220, y: 60 + Math.floor(index / 3) * 140 },
    data: { label: path.split('/').pop() ?? path },
    style: { ...nodeStyle },
  }
}

// One live observer per workspace; tied to the current root handle.
let activeObserver: ReturnType<typeof createFsObserver> = null

function stopObserver() {
  if (activeObserver) {
    try { activeObserver.disconnect() } catch { /* ignore */ }
    activeObserver = null
  }
}

export const useWorkspace = create<WorkspaceState>((set, get) => {
  // Re-read a file that changed on disk if it's currently open and clean.
  // (If the user has unsaved edits, we leave them alone — last-write-wins on save.)
  const syncOpenFileFromDisk = async (path: string) => {
    const file = get().openFiles[path]
    if (!file || file.dirty) return
    try {
      const fresh = await readFileText(file.handle)
      if (fresh === file.contents) return
      set((s) => {
        const current = s.openFiles[path]
        if (!current || current.dirty) return {}
        return {
          openFiles: { ...s.openFiles, [path]: { ...current, contents: fresh } },
        }
      })
    } catch {
      // file may have just been deleted; tree refresh will pick it up
    }
  }

  const startObserver = async (root: FsEntry) => {
    stopObserver()
    const rootHandle = root.handle as FileSystemDirectoryHandle
    const rootName = root.name

    let pendingTreeRefresh: ReturnType<typeof setTimeout> | null = null
    const scheduleTreeRefresh = () => {
      if (pendingTreeRefresh) return
      pendingTreeRefresh = setTimeout(() => {
        pendingTreeRefresh = null
        void get().refreshTree()
      }, 50)
    }

    const observer = createFsObserver((records: FsChangeRecord[]) => {
      let structural = false
      for (const r of records) {
        const path = [rootName, ...r.relativePathComponents].join('/')
        if (r.type === 'modified') {
          void syncOpenFileFromDisk(path)
        } else if (r.type === 'disappeared') {
          structural = true
          // close any open file that lived under the removed path
          const open = get().openFiles
          for (const p of Object.keys(open)) {
            if (p === path || p.startsWith(`${path}/`)) {
              set((s) => {
                const { [p]: _, ...rest } = s.openFiles
                const order = s.tabOrder.filter((tp) => tp !== p)
                let nextActive = s.activePath
                if (s.activePath === p) {
                  const idx = s.tabOrder.indexOf(p)
                  nextActive = order[Math.min(idx, order.length - 1)] ?? null
                }
                return { openFiles: rest, tabOrder: order, activePath: nextActive }
              })
            }
          }
        } else {
          structural = true
        }
      }
      if (structural) scheduleTreeRefresh()
    })

    if (!observer) return
    try {
      await observer.observe(rootHandle, { recursive: true })
      activeObserver = observer
    } catch (err) {
      console.warn('FileSystemObserver.observe failed:', err)
    }
  }

  const adoptRoot = async (handle: FileSystemDirectoryHandle) => {
    const tree = await readDirectoryTree(handle)
    set({ root: tree, rootStatus: 'connected' })
    await startObserver(tree)
  }

  return {
    root: null,
    rootStatus: 'idle',
    openFiles: {},
    tabOrder: [],
    activePath: null,
    recentlyClosed: [],
    cursor: null,
    pendingCursor: null,
    nodes: [],
    edges: [],

    openRoot: async (handle) => {
      // Fresh workspace: drop previous tabs/canvas state.
      stopObserver()
      set({ openFiles: {}, tabOrder: [], activePath: null, recentlyClosed: [], nodes: [], edges: [] })
      await idbSet(ROOT_HANDLE_KEY, handle)
      await adoptRoot(handle)
    },

    restoreRoot: async () => {
      const handle = await idbGet<FileSystemDirectoryHandle>(ROOT_HANDLE_KEY)
      if (!handle) return
      const perm = await queryHandlePermission(handle)
      if (perm === 'granted') {
        await adoptRoot(handle)
        return
      }
      if (perm === 'denied') {
        await idbDel(ROOT_HANDLE_KEY)
        set({ rootStatus: 'denied' })
        return
      }
      // 'prompt' — preserve handle in state so UI can offer to re-grant via a user gesture
      set({
        root: {
          name: handle.name,
          path: handle.name,
          kind: 'directory',
          handle,
          children: [],
        },
        rootStatus: 'needs-permission',
      })
    },

    requestPermission: async () => {
      const root = get().root
      if (!root) return
      const handle = root.handle as FileSystemDirectoryHandle
      const result = await requestHandlePermission(handle)
      if (result === 'granted') await adoptRoot(handle)
      else if (result === 'denied') {
        await idbDel(ROOT_HANDLE_KEY)
        set({ root: null, rootStatus: 'denied' })
      }
    },

    closeRoot: async () => {
      stopObserver()
      await idbDel(ROOT_HANDLE_KEY)
      set({
        root: null,
        rootStatus: 'idle',
        openFiles: {},
        tabOrder: [],
        activePath: null,
        recentlyClosed: [],
        cursor: null,
        nodes: [],
        edges: [],
      })
    },

    refreshTree: async () => {
      const { root } = get()
      if (!root) return
      const fresh = await readDirectoryTree(root.handle as FileSystemDirectoryHandle)
      set({ root: fresh })
    },

    openFile: async (entry) => {
      if (entry.kind !== 'file') return
      const existing = get().openFiles[entry.path]
      if (existing) {
        set({ activePath: entry.path })
        return
      }
      const handle = entry.handle as FileSystemFileHandle
      const contents = await readFileText(handle)
      set((s) => {
        const hasNode = s.nodes.some((n) => n.id === entry.path)
        return {
          openFiles: {
            ...s.openFiles,
            [entry.path]: { path: entry.path, handle, contents, dirty: false },
          },
          tabOrder: s.tabOrder.includes(entry.path) ? s.tabOrder : [...s.tabOrder, entry.path],
          activePath: entry.path,
          nodes: hasNode ? s.nodes : [...s.nodes, buildNodeForPath(entry.path, s.nodes.length)],
        }
      })
    },

    closeFile: (path) =>
      set((s) => {
        const file = s.openFiles[path]
        if (!file) return {}
        if (file.dirty) {
          const ok = window.confirm(`Discard unsaved changes to ${path}?`)
          if (!ok) return {}
        }
        const { [path]: _gone, ...rest } = s.openFiles
        const order = s.tabOrder.filter((p) => p !== path)
        let nextActive = s.activePath
        if (s.activePath === path) {
          const idx = s.tabOrder.indexOf(path)
          nextActive = order[Math.min(idx, order.length - 1)] ?? null
        }
        const closed = [{ path, handle: file.handle }, ...s.recentlyClosed.filter((c) => c.path !== path)].slice(0, 20)
        return { openFiles: rest, tabOrder: order, activePath: nextActive, recentlyClosed: closed }
      }),

    closeOthers: (path) =>
      set((s) => {
        const keep = s.openFiles[path]
        if (!keep) return {}
        const dirtyOthers = Object.values(s.openFiles).filter((f) => f.path !== path && f.dirty)
        if (dirtyOthers.length > 0) {
          const ok = window.confirm(`Discard unsaved changes in ${dirtyOthers.length} other tab(s)?`)
          if (!ok) return {}
        }
        const closedExtras = s.tabOrder
          .filter((p) => p !== path)
          .map((p) => ({ path: p, handle: s.openFiles[p].handle }))
        return {
          openFiles: { [path]: keep },
          tabOrder: [path],
          activePath: path,
          recentlyClosed: [...closedExtras, ...s.recentlyClosed].slice(0, 20),
        }
      }),

    closeAll: () =>
      set((s) => {
        const dirty = Object.values(s.openFiles).filter((f) => f.dirty)
        if (dirty.length > 0) {
          const ok = window.confirm(`Discard unsaved changes in ${dirty.length} tab(s)?`)
          if (!ok) return {}
        }
        const closed = s.tabOrder.map((p) => ({ path: p, handle: s.openFiles[p].handle }))
        return {
          openFiles: {},
          tabOrder: [],
          activePath: null,
          recentlyClosed: [...closed, ...s.recentlyClosed].slice(0, 20),
        }
      }),

    reopenClosed: async () => {
      const { recentlyClosed } = get()
      const next = recentlyClosed[0]
      if (!next) return
      set({ recentlyClosed: recentlyClosed.slice(1) })
      try {
        const contents = await readFileText(next.handle)
        set((s) => ({
          openFiles: {
            ...s.openFiles,
            [next.path]: { path: next.path, handle: next.handle, contents, dirty: false },
          },
          tabOrder: s.tabOrder.includes(next.path) ? s.tabOrder : [...s.tabOrder, next.path],
          activePath: next.path,
        }))
      } catch {
        // file may have been deleted on disk; just drop it
      }
    },

    setActive: (path) => set({ activePath: path }),

    cycleTab: (delta) => {
      const { tabOrder, activePath } = get()
      if (tabOrder.length === 0) return
      const idx = activePath ? tabOrder.indexOf(activePath) : -1
      const next = (idx + delta + tabOrder.length) % tabOrder.length
      set({ activePath: tabOrder[next] })
    },

    activateTabByIndex: (index) => {
      const { tabOrder } = get()
      const target = tabOrder[index]
      if (target) set({ activePath: target })
    },

    setCursor: (cursor) => set({ cursor }),

    revealAt: async (entry, line, column = 1) => {
      await get().openFile(entry)
      set((s) => ({
        pendingCursor: {
          path: entry.path,
          line,
          column,
          token: (s.pendingCursor?.token ?? 0) + 1,
        },
      }))
    },

    revealActive: (line, column = 1) => {
      const path = get().activePath
      if (!path) return
      set((s) => ({
        pendingCursor: {
          path,
          line,
          column,
          token: (s.pendingCursor?.token ?? 0) + 1,
        },
      }))
    },

    updateContents: (path, contents) =>
      set((s) => {
        const file = s.openFiles[path]
        if (!file) return {}
        return {
          openFiles: {
            ...s.openFiles,
            [path]: { ...file, contents, dirty: contents !== file.contents || file.dirty },
          },
        }
      }),

    saveActive: async () => {
      const { activePath, openFiles } = get()
      if (!activePath) return
      const file = openFiles[activePath]
      if (!file) return
      const formatOnSave = useSettings.getState().formatOnSave
      const formatted = formatOnSave ? applyFormat(file.contents) : file.contents
      await writeFileText(file.handle, formatted)
      set((s) => ({
        openFiles: {
          ...s.openFiles,
          [activePath]: { ...file, contents: formatted, dirty: false },
        },
      }))
    },

    saveAll: async () => {
      const formatOnSave = useSettings.getState().formatOnSave
      const files = Object.values(get().openFiles).filter((f) => f.dirty)
      const updates = files.map((f) => ({
        path: f.path,
        formatted: formatOnSave ? applyFormat(f.contents) : f.contents,
        file: f,
      }))
      await Promise.all(updates.map((u) => writeFileText(u.file.handle, u.formatted)))
      set((s) => {
        const next = { ...s.openFiles }
        for (const u of updates) next[u.path] = { ...next[u.path], contents: u.formatted, dirty: false }
        return { openFiles: next }
      })
    },

    createNewFile: async (name) => {
      const { root, refreshTree } = get()
      if (!root) {
        alert('Open a folder first.')
        return
      }
      const handle = await createFile(root.handle as FileSystemDirectoryHandle, name)
      await writeFileText(handle, '')
      await refreshTree()
      const path = `${root.name}/${name}`
      set((s) => ({
        openFiles: {
          ...s.openFiles,
          [path]: { path, handle, contents: '', dirty: false },
        },
        tabOrder: s.tabOrder.includes(path) ? s.tabOrder : [...s.tabOrder, path],
        activePath: path,
        nodes: [...s.nodes, buildNodeForPath(path, s.nodes.length)],
      }))
    },

    createFileIn: async (dir, name) => {
      if (dir.kind !== 'directory') return
      const trimmed = name.trim()
      if (!trimmed) return
      const handle = await createFile(dir.handle as FileSystemDirectoryHandle, trimmed)
      await writeFileText(handle, '')
      await get().refreshTree()
      const path = `${dir.path}/${trimmed}`
      set((s) => ({
        openFiles: {
          ...s.openFiles,
          [path]: { path, handle, contents: '', dirty: false },
        },
        tabOrder: s.tabOrder.includes(path) ? s.tabOrder : [...s.tabOrder, path],
        activePath: path,
      }))
    },

    createDirectoryIn: async (dir, name) => {
      if (dir.kind !== 'directory') return
      const trimmed = name.trim()
      if (!trimmed) return
      await createDirectory(dir.handle as FileSystemDirectoryHandle, trimmed)
      await get().refreshTree()
    },

    deleteEntry: async (parent, entry) => {
      if (parent.kind !== 'directory') return
      const label = entry.kind === 'directory' ? `folder “${entry.name}” and all its contents` : `file “${entry.name}”`
      if (!window.confirm(`Delete ${label}? This cannot be undone.`)) return
      await removeEntry(parent.handle as FileSystemDirectoryHandle, entry.name, entry.kind === 'directory')
      // close any open file that lived under the removed path
      set((s) => {
        const next = { ...s.openFiles }
        let order = [...s.tabOrder]
        let activePath = s.activePath
        for (const p of Object.keys(next)) {
          if (p === entry.path || p.startsWith(`${entry.path}/`)) {
            delete next[p]
            const idx = order.indexOf(p)
            order = order.filter((tp) => tp !== p)
            if (activePath === p) activePath = order[Math.min(idx, order.length - 1)] ?? null
          }
        }
        return { openFiles: next, tabOrder: order, activePath }
      })
      await get().refreshTree()
    },

    renameEntry: async (parent, entry, newName) => {
      const trimmed = newName.trim()
      if (!trimmed || trimmed === entry.name) return
      const ok = await renameHandle(entry.handle as FileSystemFileHandle | FileSystemDirectoryHandle, trimmed)
      if (!ok) {
        alert('Rename is not supported in this browser version.')
        return
      }
      const oldPath = entry.path
      const newPath = `${parent.path}/${trimmed}`
      set((s) => {
        const next: typeof s.openFiles = {}
        const order = s.tabOrder.map((p) => {
          if (p === oldPath) return newPath
          if (p.startsWith(`${oldPath}/`)) return `${newPath}${p.slice(oldPath.length)}`
          return p
        })
        for (const [p, file] of Object.entries(s.openFiles)) {
          if (p === oldPath) next[newPath] = { ...file, path: newPath }
          else if (p.startsWith(`${oldPath}/`)) {
            const remapped = `${newPath}${p.slice(oldPath.length)}`
            next[remapped] = { ...file, path: remapped }
          } else {
            next[p] = file
          }
        }
        const activePath =
          s.activePath === oldPath
            ? newPath
            : s.activePath?.startsWith(`${oldPath}/`)
              ? `${newPath}${s.activePath.slice(oldPath.length)}`
              : s.activePath
        return { openFiles: next, tabOrder: order, activePath }
      })
      await get().refreshTree()
    },

    setNodes: (updater) =>
      set((s) => ({ nodes: typeof updater === 'function' ? updater(s.nodes) : updater })),

    setEdges: (updater) =>
      set((s) => ({ edges: typeof updater === 'function' ? updater(s.edges) : updater })),
  }
})
