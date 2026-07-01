//! The signed-out gate: log in or create an author account. Also offers the
//! "keep local" escape hatch — open a folder on disk with no account.

import { useState } from 'react'
import { useAuth } from '@/store/auth'
import { useWorkspace } from '@/store/workspace'

export function AuthGate() {
  const [mode, setMode] = useState<'signin' | 'signup'>('signin')
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const error = useAuth((s) => s.error)
  const signIn = useAuth((s) => s.signIn)
  const signUp = useAuth((s) => s.signUp)
  const openRoot = useWorkspace((s) => s.openRoot)

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    setBusy(true)
    try {
      if (mode === 'signup') await signUp(name || 'Author', email, password)
      else await signIn(email, password)
    } catch {
      /* error surfaced via the store */
    } finally {
      setBusy(false)
    }
  }

  const openLocalFolder = async () => {
    const picker = (window as unknown as { showDirectoryPicker?: () => Promise<FileSystemDirectoryHandle> }).showDirectoryPicker
    if (!picker) {
      alert('This browser does not support opening a local folder. Use a Chromium browser, or sign in to use server projects.')
      return
    }
    try {
      const handle = await picker()
      await openRoot(handle)
    } catch {
      /* user cancelled */
    }
  }

  return (
    <div className="h-full w-full flex items-center justify-center bg-zinc-950 text-zinc-100">
      <div className="w-full max-w-sm rounded-2xl border border-zinc-800 bg-zinc-900 p-8 shadow-xl">
        <div className="mb-6 text-center">
          <div className="text-2xl font-semibold tracking-tight">Loom</div>
          <div className="mt-1 text-sm text-zinc-400">
            {mode === 'signin' ? 'Sign in to your events' : 'Create your author account'}
          </div>
        </div>

        <form onSubmit={submit} className="flex flex-col gap-3">
          {mode === 'signup' && (
            <input
              className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm outline-none focus:border-indigo-500"
              placeholder="Name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoComplete="name"
            />
          )}
          <input
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm outline-none focus:border-indigo-500"
            placeholder="Email"
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            autoComplete="email"
            required
          />
          <input
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm outline-none focus:border-indigo-500"
            placeholder="Password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete={mode === 'signup' ? 'new-password' : 'current-password'}
            required
            minLength={8}
          />
          {error && <div className="text-xs text-red-400">{error}</div>}
          <button
            type="submit"
            disabled={busy}
            className="mt-1 rounded-lg bg-indigo-600 px-3 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
          >
            {busy ? '…' : mode === 'signin' ? 'Sign in' : 'Create account'}
          </button>
        </form>

        <button
          onClick={() => setMode(mode === 'signin' ? 'signup' : 'signin')}
          className="mt-4 w-full text-center text-xs text-zinc-400 hover:text-zinc-200"
        >
          {mode === 'signin' ? "No account? Create one" : 'Have an account? Sign in'}
        </button>

        <div className="my-5 flex items-center gap-3 text-[10px] uppercase tracking-wider text-zinc-600">
          <div className="h-px flex-1 bg-zinc-800" />
          or
          <div className="h-px flex-1 bg-zinc-800" />
        </div>

        <button
          onClick={openLocalFolder}
          className="w-full rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-300 hover:bg-zinc-800"
        >
          Open a local folder (no account)
        </button>
      </div>
    </div>
  )
}
