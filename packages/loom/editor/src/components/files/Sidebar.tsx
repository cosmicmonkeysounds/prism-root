import { useWorkspace } from '@/store/workspace'
import { isFsAccessSupported, pickDirectory } from '@/lib/fs'
import { FileTree } from '@/components/files/FileTree'

export function Sidebar() {
  const root = useWorkspace((s) => s.root)
  const rootStatus = useWorkspace((s) => s.rootStatus)
  const openRoot = useWorkspace((s) => s.openRoot)
  const closeRoot = useWorkspace((s) => s.closeRoot)
  const requestPermission = useWorkspace((s) => s.requestPermission)
  const createNewFile = useWorkspace((s) => s.createNewFile)

  const openFolder = async () => {
    if (!isFsAccessSupported()) {
      alert('The File System Access API is not supported in this browser. Use Chrome or Edge.')
      return
    }
    try {
      const dir = await pickDirectory()
      await openRoot(dir)
    } catch (err) {
      if ((err as DOMException)?.name !== 'AbortError') console.error(err)
    }
  }

  return (
    <aside className="h-full flex flex-col border-r border-white/10 bg-zinc-950">
      <div className="px-3 py-2 border-b border-white/10 flex items-center justify-between gap-2">
        <span className="text-xs uppercase tracking-wider text-zinc-500">Explorer</span>
        <div className="flex items-center gap-1">
          {root && rootStatus === 'connected' && (
            <button
              type="button"
              onClick={() => {
                const name = window.prompt('New file name (in workspace root):')
                if (name) void createNewFile(name)
              }}
              className="text-xs text-zinc-300 hover:text-white px-2 py-0.5 border border-white/10 rounded hover:bg-white/5"
              title="New file in workspace root"
            >
              + File
            </button>
          )}
          {root && (
            <button
              type="button"
              onClick={() => void closeRoot()}
              className="text-xs text-zinc-400 hover:text-white px-2 py-0.5 border border-white/10 rounded hover:bg-white/5"
              title="Close workspace"
            >
              Close
            </button>
          )}
          <button
            type="button"
            onClick={() => void openFolder()}
            className="text-xs text-zinc-300 hover:text-white px-2 py-0.5 border border-white/10 rounded hover:bg-white/5"
          >
            Open Folder
          </button>
        </div>
      </div>
      <div className="flex-1 min-h-0">
        {rootStatus === 'needs-permission' && root ? (
          <div className="p-3 text-xs text-zinc-400 space-y-2">
            <p>
              Reconnect to <strong className="text-zinc-200">{root.name}</strong> to restore the
              workspace. The browser requires your permission again after a reload.
            </p>
            <button
              type="button"
              onClick={() => void requestPermission()}
              className="px-2 py-1 text-xs rounded border border-blue-400/40 bg-blue-500/10 text-blue-200 hover:bg-blue-500/20"
            >
              Grant access
            </button>
          </div>
        ) : root ? (
          <FileTree root={root} />
        ) : (
          <div className="p-3 text-xs text-zinc-500">
            No folder open. Click <strong className="text-zinc-300">Open Folder</strong> to grant
            access to a local directory.
          </div>
        )}
      </div>
    </aside>
  )
}
