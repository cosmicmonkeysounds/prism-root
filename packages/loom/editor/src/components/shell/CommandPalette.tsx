import { useEffect, useMemo, useRef, useState } from 'react'
import clsx from 'clsx'
import type { FsEntry } from '@/lib/fs'
import { useWorkspace } from '@/store/workspace'
import { useSettings } from '@/store/settings'
import { useMode } from '@/store/mode'
import { useEditJournal } from '@/store/edit-journal'
import { lspWorkspaceSync, uriFor } from '@/lib/lsp-client'
import { navigateToLocation } from '@/lib/lsp-nav'
import { useLspIndexGen } from '@/lib/lsp-index'

type Mode = 'files' | 'commands' | 'symbols' | 'wsymbols'

type Command = {
  id: string
  label: string
  hint?: string
  run: () => void | Promise<void>
}

type SymbolHit = {
  name: string
  kindLabel: string
  uri: string
  path: string
  line: number
  character: number
}

// LSP SymbolKind → short label (subset Loom emits; see core/src/lsp/symbols.ts).
const KIND_LABEL: Record<number, string> = {
  2: 'mod', 3: 'ns', 5: 'class', 6: 'method', 7: 'prop', 10: 'enum', 11: 'iface',
  12: 'fn', 13: 'var', 17: 'bool', 19: 'object', 22: 'struct', 23: 'event',
  24: 'operator', 25: 'array',
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

/** Document symbols for one URI, mapped to palette hits. */
function symbolsForUri(uri: string, path: string): SymbolHit[] {
  const syms = lspWorkspaceSync().documentSymbols(uri)
  if (!syms) return []
  return syms.map((s) => ({
    name: s.name,
    kindLabel: KIND_LABEL[s.kind] ?? 'decl',
    uri,
    path,
    line: s.range.start.line,
    character: s.range.start.character,
  }))
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
  // Recompute symbol lists when the LSP index changes (async project index
  // completing while the palette is open).
  const indexGen = useLspIndexGen((s) => s.gen)
  const undoTop = useEditJournal((s) => s.undoStack[s.undoStack.length - 1]?.label ?? null)
  const redoTop = useEditJournal((s) => s.redoStack[s.redoStack.length - 1]?.label ?? null)

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
      } else if (mod && e.shiftKey && e.key.toLowerCase() === 'o') {
        // ⌘/Ctrl+Shift+O — go to symbol in file.
        e.preventDefault()
        setOpen(true)
        setMode('symbols')
        setQuery('@')
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

  // The leading sigil selects the mode: `>` commands, `@` file symbols,
  // `#` workspace symbols, else the file picker.
  const sigil = query[0]
  const effectiveMode: Mode =
    sigil === '>' ? 'commands' : sigil === '@' ? 'symbols' : sigil === '#' ? 'wsymbols' : mode
  const trimmed = ['>', '@', '#'].includes(sigil ?? '') ? query.slice(1).trim() : query.trim()

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
      { id: 'goto-symbol', label: 'Go to Symbol in File…', hint: '⌘⇧O', run: () => setQuery('@') },
      { id: 'goto-wsymbol', label: 'Go to Symbol in Workspace…', run: () => setQuery('#') },
      {
        id: 'undo-story',
        label: undoTop !== null ? `Edit: Undo Story Edit — ${undoTop}` : 'Edit: Undo Story Edit',
        hint: '⌘Z',
        run: () => void useEditJournal.getState().undo(),
      },
      {
        id: 'redo-story',
        label: redoTop !== null ? `Edit: Redo Story Edit — ${redoTop}` : 'Edit: Redo Story Edit',
        hint: '⌘⇧Z',
        run: () => void useEditJournal.getState().redo(),
      },
      { id: 'toggle-rail', label: 'View: Toggle Left Rail', hint: '⌘B', run: () => useMode.getState().toggleRail() },
      { id: 'toggle-tray', label: 'View: Toggle Properties Tray', hint: '⌘⌥B', run: () => useMode.getState().toggleTray() },
      {
        id: 'toggle-graph',
        label: 'View: Toggle Story Graph Pane',
        hint: '⌘\\',
        run: () => {
          const m = useMode.getState()
          if (m.mode === 'writing') m.setUi('writing', { graphOpen: !m.ui.writing.graphOpen })
        },
      },
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
    [activePath, saveActive, saveAll, closeFile, closeAll, reopenClosed, cycleTab, wordWrap, theme, setSetting, undoTop, redoTop],
  )

  const files = useMemo(() => (root ? flatten(root) : []), [root])

  // Symbols for the current mode. Only computed when the palette is open and
  // in a symbol mode, so we never walk the LSP index needlessly.
  const symbols = useMemo<SymbolHit[]>(() => {
    // `indexGen` is read (not just a dep) so this recomputes when the LSP
    // index version changes — the symbol data comes from the mutable
    // Workspace via documentSymbols, which React can't otherwise track.
    // `indexGen` only ever increases from 0, so `< 0` is always false.
    if (!isOpen || indexGen < 0) return []
    if (effectiveMode === 'symbols') {
      return activePath ? symbolsForUri(uriFor(activePath), activePath) : []
    }
    if (effectiveMode === 'wsymbols') {
      const out: SymbolHit[] = []
      for (const f of files) {
        if (!f.path.endsWith('.loom')) continue
        out.push(...symbolsForUri(uriFor(f.path), f.path))
      }
      return out
    }
    return []
  }, [isOpen, effectiveMode, activePath, files, indexGen])

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

  const symbolResults = useMemo(() => {
    return symbols
      .map((sym) => ({ sym, s: score(trimmed, sym.name) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 50)
      .map((x) => x.sym)
  }, [symbols, trimmed])

  if (!isOpen) return null

  const results:
    | { kind: 'file'; entry: FsEntry }[]
    | { kind: 'cmd'; cmd: Command }[]
    | { kind: 'sym'; sym: SymbolHit }[] =
    effectiveMode === 'commands'
      ? commandResults.map((c) => ({ kind: 'cmd' as const, cmd: c }))
      : effectiveMode === 'symbols' || effectiveMode === 'wsymbols'
        ? symbolResults.map((sym) => ({ kind: 'sym' as const, sym }))
        : fileResults.map((f) => ({ kind: 'file' as const, entry: f }))

  const submit = (idx: number) => {
    const pick = results[idx]
    if (!pick) return
    if (pick.kind === 'file') void openFile(pick.entry)
    else if (pick.kind === 'cmd') void pick.cmd.run()
    else {
      void navigateToLocation({
        uri: pick.sym.uri,
        range: {
          start: { line: pick.sym.line, character: pick.sym.character },
          end: { line: pick.sym.line, character: pick.sym.character },
        },
      })
    }
    // Keep the palette open when a command just rewrote the query (e.g. the
    // "Go to Symbol…" commands switch mode in place).
    if (pick.kind === 'cmd' && (pick.cmd.id === 'goto-symbol' || pick.cmd.id === 'goto-wsymbol')) {
      setSelected(0)
      return
    }
    setOpen(false)
  }

  const placeholder =
    effectiveMode === 'commands'
      ? 'Run command…'
      : effectiveMode === 'symbols'
        ? 'Go to symbol in file…'
        : effectiveMode === 'wsymbols'
          ? 'Go to symbol in workspace…'
          : 'Go to file…  (> commands · @ symbols · # workspace symbols)'

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
          placeholder={placeholder}
          className="w-full px-4 py-3 bg-transparent text-sm text-white outline-none border-b border-white/10"
        />
        <ul className="max-h-80 overflow-auto">
          {results.length === 0 && (
            <li className="px-4 py-3 text-xs text-zinc-500">No matches</li>
          )}
          {results.map((r, i) => (
            <li
              key={r.kind === 'file' ? r.entry.path : r.kind === 'cmd' ? r.cmd.id : `${r.sym.uri}:${r.sym.line}:${r.sym.name}`}
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
              ) : r.kind === 'cmd' ? (
                <>
                  <span className="truncate">{r.cmd.label}</span>
                  {r.cmd.hint && <span className="text-xs text-zinc-500">{r.cmd.hint}</span>}
                </>
              ) : (
                <>
                  <span className="truncate">
                    <span className="text-amber-400/80 mr-2 text-xs uppercase">{r.sym.kindLabel}</span>
                    {r.sym.name}
                  </span>
                  <span className="text-xs text-zinc-500 truncate">
                    {effectiveMode === 'wsymbols' ? `${r.sym.path}:${r.sym.line + 1}` : r.sym.line + 1}
                  </span>
                </>
              )}
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}
