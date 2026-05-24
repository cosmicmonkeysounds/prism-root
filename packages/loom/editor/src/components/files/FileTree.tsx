import { useEffect, useState } from 'react'
import clsx from 'clsx'
import type { FsEntry } from '@/lib/fs'
import { useWorkspace } from '@/store/workspace'

type MenuState = {
  x: number
  y: number
  entry: FsEntry
  parent: FsEntry | null
}

type Props = {
  entry: FsEntry
  parent: FsEntry | null
  depth?: number
  onContextMenu: (m: MenuState) => void
  renameTarget: string | null
  onRenameSubmit: (parent: FsEntry | null, entry: FsEntry, newName: string) => void
  onRenameCancel: () => void
}

function NodeRow({
  entry,
  parent,
  depth = 0,
  onContextMenu,
  renameTarget,
  onRenameSubmit,
  onRenameCancel,
}: Props) {
  const [open, setOpen] = useState(depth < 1)
  const openFile = useWorkspace((s) => s.openFile)
  const activePath = useWorkspace((s) => s.activePath)
  const isActive = activePath === entry.path
  const isRenaming = renameTarget === entry.path

  const renameInput = isRenaming ? (
    <input
      autoFocus
      defaultValue={entry.name}
      onBlur={(e) => onRenameSubmit(parent, entry, e.currentTarget.value)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault()
          onRenameSubmit(parent, entry, (e.target as HTMLInputElement).value)
        } else if (e.key === 'Escape') {
          e.preventDefault()
          onRenameCancel()
        }
      }}
      className="flex-1 px-1 bg-zinc-800 border border-blue-400/60 rounded text-white text-sm outline-none"
    />
  ) : null

  if (entry.kind === 'directory') {
    return (
      <div>
        <div
          className="w-full flex items-center gap-1 text-sm text-zinc-300 hover:bg-white/5"
          style={{ paddingLeft: 8 + depth * 12 }}
          onContextMenu={(e) => {
            e.preventDefault()
            onContextMenu({ x: e.clientX, y: e.clientY, entry, parent })
          }}
        >
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            className="flex items-center gap-1 flex-1 text-left py-0.5"
          >
            <span className="text-zinc-500 w-3 inline-block">{open ? '▾' : '▸'}</span>
            {!isRenaming && <span className="truncate">{entry.name}</span>}
          </button>
          {renameInput}
        </div>
        {open &&
          entry.children?.map((child) => (
            <NodeRow
              key={child.path}
              entry={child}
              parent={entry}
              depth={depth + 1}
              onContextMenu={onContextMenu}
              renameTarget={renameTarget}
              onRenameSubmit={onRenameSubmit}
              onRenameCancel={onRenameCancel}
            />
          ))}
      </div>
    )
  }

  return (
    <div
      className={clsx(
        'w-full flex items-center gap-1 text-sm truncate',
        isActive ? 'bg-white/10 text-white' : 'text-zinc-400 hover:bg-white/5',
      )}
      style={{ paddingLeft: 8 + depth * 12 + 16 }}
      onContextMenu={(e) => {
        e.preventDefault()
        onContextMenu({ x: e.clientX, y: e.clientY, entry, parent })
      }}
    >
      {isRenaming ? (
        renameInput
      ) : (
        <button
          type="button"
          onClick={() => void openFile(entry)}
          className="flex-1 text-left py-0.5 truncate"
        >
          {entry.name}
        </button>
      )}
    </div>
  )
}

export function FileTree({ root }: { root: FsEntry }) {
  const createFileIn = useWorkspace((s) => s.createFileIn)
  const createDirectoryIn = useWorkspace((s) => s.createDirectoryIn)
  const deleteEntryAction = useWorkspace((s) => s.deleteEntry)
  const renameEntryAction = useWorkspace((s) => s.renameEntry)

  const [menu, setMenu] = useState<MenuState | null>(null)
  const [renameTarget, setRenameTarget] = useState<string | null>(null)

  useEffect(() => {
    if (!menu) return
    const dismiss = () => setMenu(null)
    window.addEventListener('click', dismiss)
    window.addEventListener('contextmenu', dismiss, { capture: true })
    return () => {
      window.removeEventListener('click', dismiss)
      window.removeEventListener('contextmenu', dismiss, { capture: true })
    }
  }, [menu])

  const submitRename = (parent: FsEntry | null, entry: FsEntry, newName: string) => {
    setRenameTarget(null)
    if (!parent || !newName || newName === entry.name) return
    void renameEntryAction(parent, entry, newName)
  }

  const actions = (m: MenuState) => {
    const items: { label: string; run: () => void; danger?: boolean }[] = []
    if (m.entry.kind === 'directory') {
      items.push({
        label: 'New File…',
        run: () => {
          const name = window.prompt('New file name:')
          if (name) void createFileIn(m.entry, name)
        },
      })
      items.push({
        label: 'New Folder…',
        run: () => {
          const name = window.prompt('New folder name:')
          if (name) void createDirectoryIn(m.entry, name)
        },
      })
    }
    if (m.parent) {
      items.push({ label: 'Rename…', run: () => setRenameTarget(m.entry.path) })
      items.push({
        label: 'Delete',
        danger: true,
        run: () => void deleteEntryAction(m.parent!, m.entry),
      })
    }
    items.push({
      label: 'Copy path',
      run: () => void navigator.clipboard?.writeText(m.entry.path),
    })
    return items
  }

  return (
    <div className="overflow-auto h-full py-1">
      <NodeRow
        entry={root}
        parent={null}
        onContextMenu={setMenu}
        renameTarget={renameTarget}
        onRenameSubmit={submitRename}
        onRenameCancel={() => setRenameTarget(null)}
      />
      {menu && (
        <div
          className="fixed z-50 min-w-[180px] bg-zinc-900 border border-white/10 rounded-md shadow-lg text-xs py-1"
          style={{ left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          {actions(menu).map((item) => (
            <button
              key={item.label}
              type="button"
              onClick={() => {
                item.run()
                setMenu(null)
              }}
              className={clsx(
                'w-full text-left px-3 py-1.5 hover:bg-white/10',
                item.danger ? 'text-red-300' : 'text-zinc-200',
              )}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
