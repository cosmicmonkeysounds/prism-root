import { useEffect, useMemo, useRef, useState } from 'react'
import clsx from 'clsx'
import type { FsEntry } from '@/lib/fs'
import { useWorkspace } from '@/store/workspace'
import { useSettings } from '@/store/settings'

type Mode = 'files' | 'commands'

type Command = {
  id: string
  label: string
  hint?: string
  run: () => void | Promise<void>
}

function flatten(entry: FsEntry, out: FsEntry[] = []): FsEntry[] {
  if (entry.kind === 'file') out.push(entry)
  else entry.children?.forEach((c) => flatten(c, out))
  return out
}

function score(query: string, haystack: string): number {
  if (!query) return 1
  const q = query.toLowerCase()
  const n = haystack.toLowerCase()
  if (n === q) return 1000
  if (n.startsWith(q)) return 500
  if (n.includes(q)) return 250
  let qi = 0
  for (let i = 0; i < n.length && qi < q.length; i++) if (n[i] === q[qi]) qi++
  return qi === q.length ? 100 - (n.length - q.length) : 0
}

export function CommandPalette() {
  const root = useWorkspace((s) => s.root)
  const openFile = useWorkspace((s) => s.openFile)
  const saveActive = useWorkspace((s) => s.saveActive)
  const saveAll = useWorkspace((s) => s.saveAll)
  const closeFile = useWorkspace((s) => s.closeFile)
  const closeAll = useWorkspace((s) => s.closeAll)
  const reopenClosed = useWorkspace((s) => s.reopenClosed)
  const cycleTab = useWorkspace((s) => s.cycleTab)
  const activePath = useWorkspace((s) => s.activePath)
  const setSetting = useSettings((s) => s.set)
  const wordWrap = useSettings((s) => s.wordWrap)
  const theme = useSettings((s) => s.theme)

  const [isOpen, setOpen] = useState(false)
  const [mode, setMode] = useState<Mode>('files')
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey
      if (mod && e.key.toLowerCase() === 'p') {
        e.preventDefault()
        setOpen(true)
        setMode(e.shiftKey ? 'commands' : 'files')
        setQuery(e.shiftKey ? '>' : '')
        setSelected(0)
      } else if (e.key === 'Escape') {
        setOpen(false)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  useEffect(() => {
    if (isOpen) requestAnimationFrame(() => inputRef.current?.focus())
  }, [isOpen])

  const trimmed = query.startsWith('>') ? query.slice(1).trim() : query
  const effectiveMode: Mode = query.startsWith('>') ? 'commands' : mode

  const commands: Command[] = useMemo(
    () => [
      { id: 'save', label: 'File: Save', hint: '⌘S', run: () => saveActive() },
      { id: 'save-all', label: 'File: Save All', hint: '⌘⇧S', run: () => saveAll() },
      {
        id: 'close-tab',
        label: 'File: Close Active Tab',
        hint: '⌥W',
        run: () => {
          if (activePath) closeFile(activePath)
        },
      },
      { id: 'close-all', label: 'File: Close All Tabs', run: () => closeAll() },
      { id: 'reopen-closed', label: 'File: Reopen Closed Tab', hint: '⌥⇧T', run: () => reopenClosed() },
      { id: 'next-tab', label: 'View: Next Tab', hint: '⌥]', run: () => cycleTab(1) },
      { id: 'prev-tab', label: 'View: Previous Tab', hint: '⌥[', run: () => cycleTab(-1) },
      {
        id: 'toggle-wrap',
        label: `View: ${wordWrap ? 'Disable' : 'Enable'} Word Wrap`,
        run: () => setSetting('wordWrap', !wordWrap),
      },
      {
        id: 'toggle-theme',
        label: `View: Switch to ${theme === 'dark' ? 'Light' : 'Dark'} Theme`,
        run: () => setSetting('theme', theme === 'dark' ? 'light' : 'dark'),
      },
      {
        id: 'open-settings',
        label: 'Preferences: Open Settings',
        hint: '⌘,',
        run: () => {
          // Dispatch a synthetic Cmd+, to be picked up by SettingsPanel.
          window.dispatchEvent(
            new KeyboardEvent('keydown', { key: ',', metaKey: true, bubbles: true }),
          )
        },
      },
    ],
    [activePath, saveActive, saveAll, closeFile, closeAll, reopenClosed, cycleTab, wordWrap, theme, setSetting],
  )

  const files = useMemo(() => (root ? flatten(root) : []), [root])

  const fileResults = useMemo(() => {
    return files
      .map((f) => ({ f, s: score(trimmed, f.path) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 30)
      .map((x) => x.f)
  }, [files, trimmed])

  const commandResults = useMemo(() => {
    return commands
      .map((c) => ({ c, s: score(trimmed, c.label) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .map((x) => x.c)
  }, [commands, trimmed])

  if (!isOpen) return null

  const results: ({ kind: 'file'; entry: FsEntry } | { kind: 'cmd'; cmd: Command })[] =
    effectiveMode === 'commands'
      ? commandResults.map((c) => ({ kind: 'cmd' as const, cmd: c }))
      : fileResults.map((f) => ({ kind: 'file' as const, entry: f }))

  const submit = (idx: number) => {
    const pick = results[idx]
    if (!pick) return
    if (pick.kind === 'file') void openFile(pick.entry)
    else void pick.cmd.run()
    setOpen(false)
  }

  return (
    <div
      className="fixed inset-0 z-50 bg-black/40 flex items-start justify-center pt-24"
      onClick={() => setOpen(false)}
    >
      <div
        className="w-[560px] max-w-[92vw] bg-zinc-900 border border-white/10 rounded-lg shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value)
            setSelected(0)
          }}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown') {
              e.preventDefault()
              setSelected((i) => Math.min(results.length - 1, i + 1))
            } else if (e.key === 'ArrowUp') {
              e.preventDefault()
              setSelected((i) => Math.max(0, i - 1))
            } else if (e.key === 'Enter') {
              e.preventDefault()
              submit(selected)
            }
          }}
          placeholder={effectiveMode === 'commands' ? 'Run command…' : 'Go to file…  (prefix with > for commands)'}
          className="w-full px-4 py-3 bg-transparent text-sm text-white outline-none border-b border-white/10"
        />
        <ul className="max-h-80 overflow-auto">
          {results.length === 0 && (
            <li className="px-4 py-3 text-xs text-zinc-500">No matches</li>
          )}
          {results.map((r, i) => (
            <li
              key={r.kind === 'file' ? r.entry.path : r.cmd.id}
              onMouseEnter={() => setSelected(i)}
              onClick={() => submit(i)}
              className={clsx(
                'px-4 py-1.5 text-sm cursor-pointer flex justify-between gap-3',
                i === selected ? 'bg-white/10 text-white' : 'text-zinc-300',
              )}
            >
              {r.kind === 'file' ? (
                <>
                  <span className="truncate">{r.entry.name}</span>
                  <span className="text-xs text-zinc-500 truncate">{r.entry.path}</span>
                </>
              ) : (
                <>
                  <span className="truncate">{r.cmd.label}</span>
                  {r.cmd.hint && <span className="text-xs text-zinc-500">{r.cmd.hint}</span>}
                </>
              )}
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}
