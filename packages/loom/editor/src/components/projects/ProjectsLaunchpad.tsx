//! The signed-in home: pick a project to open, create a new one, or open a
//! local folder. Opening a project enters the Studio shell editing its files.

import { useEffect, useState } from 'react'
import { useAuth } from '@/store/auth'
import { useProjects } from '@/store/projects'
import { useWorkspace } from '@/store/workspace'

const TEMPLATES = [
  { id: 'escape-the-internet', label: 'Escape the Internet (example)' },
  { id: 'blank', label: 'Blank project' },
]

export function ProjectsLaunchpad() {
  const user = useAuth((s) => s.user)
  const signOut = useAuth((s) => s.signOut)
  const { projects, status, error, load, create, open, remove } = useProjects()
  const openRoot = useWorkspace((s) => s.openRoot)

  const [newName, setNewName] = useState('')
  const [template, setTemplate] = useState(TEMPLATES[0].id)
  const [busy, setBusy] = useState<string | null>(null)

  useEffect(() => {
    void load()
  }, [load])

  const doCreate = async () => {
    const name = newName.trim() || 'Untitled Project'
    setBusy('create')
    try {
      const p = await create(name, template)
      setNewName('')
      await open(p.id)
    } finally {
      setBusy(null)
    }
  }

  const doOpen = async (id: string) => {
    setBusy(id)
    try {
      await open(id)
    } finally {
      setBusy(null)
    }
  }

  const openLocalFolder = async () => {
    const picker = (window as unknown as { showDirectoryPicker?: () => Promise<FileSystemDirectoryHandle> }).showDirectoryPicker
    if (!picker) return
    try {
      await openRoot(await picker())
    } catch {
      /* cancelled */
    }
  }

  return (
    <div className="h-full w-full overflow-auto bg-zinc-950 text-zinc-100">
      <div className="mx-auto max-w-3xl px-6 py-10">
        <header className="mb-8 flex items-center justify-between">
          <div>
            <h1 className="text-xl font-semibold tracking-tight">Your projects</h1>
            <p className="text-sm text-zinc-400">{user?.email}</p>
          </div>
          <div className="flex items-center gap-2">
            <button onClick={openLocalFolder} className="rounded-lg border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 hover:bg-zinc-800">
              Open local folder
            </button>
            <button onClick={() => void signOut()} className="rounded-lg px-3 py-1.5 text-sm text-zinc-400 hover:text-zinc-200">
              Sign out
            </button>
          </div>
        </header>

        <div className="mb-8 flex flex-wrap items-center gap-2 rounded-xl border border-zinc-800 bg-zinc-900 p-3">
          <input
            className="min-w-[12rem] flex-1 rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm outline-none focus:border-indigo-500"
            placeholder="New project name"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && void doCreate()}
          />
          <select
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm outline-none focus:border-indigo-500"
            value={template}
            onChange={(e) => setTemplate(e.target.value)}
          >
            {TEMPLATES.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}
              </option>
            ))}
          </select>
          <button
            onClick={() => void doCreate()}
            disabled={busy === 'create'}
            className="rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
          >
            {busy === 'create' ? 'Creating…' : 'Create'}
          </button>
        </div>

        {status === 'loading' && <div className="text-sm text-zinc-500">Loading…</div>}
        {status === 'error' && <div className="text-sm text-red-400">{error}</div>}
        {status === 'ready' && projects.length === 0 && (
          <div className="rounded-xl border border-dashed border-zinc-800 p-8 text-center text-sm text-zinc-500">
            No projects yet — create one above to get started.
          </div>
        )}

        <ul className="flex flex-col gap-2">
          {projects.map((p) => (
            <li
              key={p.id}
              className="flex items-center justify-between rounded-xl border border-zinc-800 bg-zinc-900 px-4 py-3 hover:border-zinc-700"
            >
              <button className="flex-1 text-left" onClick={() => void doOpen(p.id)} disabled={busy === p.id}>
                <div className="flex items-center gap-2">
                  <span className="font-medium">{p.name}</span>
                  {p.activeEvent && (
                    <span
                      className={`rounded-full px-2 py-0.5 text-[10px] uppercase tracking-wide ${
                        p.activeEvent.status === 'open' ? 'bg-emerald-900 text-emerald-300' : 'bg-amber-900 text-amber-300'
                      }`}
                    >
                      {p.activeEvent.mode} · {p.activeEvent.status}
                    </span>
                  )}
                </div>
                <div className="text-xs text-zinc-500">
                  {busy === p.id ? 'Opening…' : `updated ${new Date(p.updatedAt).toLocaleString()}`}
                </div>
              </button>
              <button
                onClick={() => {
                  if (window.confirm(`Delete “${p.name}”? This cannot be undone.`)) void remove(p.id)
                }}
                className="ml-3 rounded-lg px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-800 hover:text-red-400"
              >
                Delete
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}
