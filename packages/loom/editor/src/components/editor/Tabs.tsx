import { useEffect, useState } from 'react'
import clsx from 'clsx'
import { useWorkspace } from '@/store/workspace'

export function Tabs() {
  const openFiles = useWorkspace((s) => s.openFiles)
  const tabOrder = useWorkspace((s) => s.tabOrder)
  const activePath = useWorkspace((s) => s.activePath)
  const setActive = useWorkspace((s) => s.setActive)
  const closeFile = useWorkspace((s) => s.closeFile)
  const closeOthers = useWorkspace((s) => s.closeOthers)
  const closeAll = useWorkspace((s) => s.closeAll)
  const cycleTab = useWorkspace((s) => s.cycleTab)
  const activateTabByIndex = useWorkspace((s) => s.activateTabByIndex)
  const reopenClosed = useWorkspace((s) => s.reopenClosed)

  const [menu, setMenu] = useState<{ path: string; x: number; y: number } | null>(null)

  useEffect(() => {
    // Shortcuts chosen to avoid Chrome reservations (Cmd+W/T/1..9 are owned by the browser).
    const onKey = (e: KeyboardEvent) => {
      // Alt+W — close active tab
      if (e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === 'w') {
        if (!activePath) return
        e.preventDefault()
        closeFile(activePath)
        return
      }
      // Alt+Shift+T — reopen closed tab
      if (e.altKey && e.shiftKey && !e.metaKey && !e.ctrlKey && e.key.toLowerCase() === 't') {
        e.preventDefault()
        void reopenClosed()
        return
      }
      // Alt+] / Alt+[ — next/prev tab
      if (e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey && (e.key === ']' || e.key === '[')) {
        e.preventDefault()
        cycleTab(e.key === ']' ? 1 : -1)
        return
      }
      // Alt+1..9 — switch tabs
      if (e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey && /^[1-9]$/.test(e.key)) {
        if (tabOrder.length === 0) return
        e.preventDefault()
        activateTabByIndex(Number(e.key) - 1)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [activePath, closeFile, reopenClosed, cycleTab, activateTabByIndex, tabOrder.length])

  useEffect(() => {
    if (!menu) return
    const dismiss = () => setMenu(null)
    window.addEventListener('click', dismiss)
    window.addEventListener('contextmenu', dismiss)
    return () => {
      window.removeEventListener('click', dismiss)
      window.removeEventListener('contextmenu', dismiss)
    }
  }, [menu])

  if (tabOrder.length === 0) return null

  return (
    <div className="flex items-stretch border-b border-white/10 bg-zinc-900/50 overflow-x-auto select-none">
      {tabOrder.map((path) => {
        const file = openFiles[path]
        if (!file) return null
        const name = path.split('/').pop()
        const active = activePath === path
        return (
          <div
            key={path}
            className={clsx(
              'group flex items-center gap-2 px-3 py-1.5 text-xs cursor-pointer border-r border-white/5',
              active ? 'bg-zinc-950 text-white' : 'text-zinc-400 hover:bg-white/5',
            )}
            title={path}
            onClick={() => setActive(path)}
            onAuxClick={(e) => {
              // middle click closes
              if (e.button === 1) {
                e.preventDefault()
                closeFile(path)
              }
            }}
            onContextMenu={(e) => {
              e.preventDefault()
              setMenu({ path, x: e.clientX, y: e.clientY })
            }}
          >
            <span className="truncate max-w-[180px]">{name}</span>
            <button
              type="button"
              className={clsx(
                'rounded hover:bg-white/10 px-1 -mr-1',
                file.dirty ? 'text-blue-400' : 'text-zinc-500 hover:text-zinc-100',
              )}
              onClick={(e) => {
                e.stopPropagation()
                closeFile(path)
              }}
              aria-label={file.dirty ? 'Unsaved — click to close' : 'Close'}
            >
              {file.dirty ? '●' : '×'}
            </button>
          </div>
        )
      })}
      <button
        type="button"
        onClick={closeAll}
        className="ml-auto px-2 text-[11px] text-zinc-500 hover:text-zinc-200"
        title="Close all tabs"
      >
        Close all
      </button>

      {menu && (
        <div
          className="fixed z-50 min-w-[180px] bg-zinc-900 border border-white/10 rounded-md shadow-lg text-xs py-1"
          style={{ left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          {[
            { label: 'Close', action: () => closeFile(menu.path) },
            { label: 'Close others', action: () => closeOthers(menu.path) },
            { label: 'Close all', action: () => closeAll() },
            { label: 'Reopen closed tab', action: () => void reopenClosed() },
          ].map((item) => (
            <button
              key={item.label}
              type="button"
              onClick={() => {
                item.action()
                setMenu(null)
              }}
              className="w-full text-left px-3 py-1.5 hover:bg-white/10 text-zinc-200"
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
