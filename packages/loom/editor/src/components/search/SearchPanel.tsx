import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import clsx from 'clsx'
import { useWorkspace } from '@/store/workspace'
import type { FsEntry } from '@/lib/fs'

type Match = {
  path: string
  entry: FsEntry
  line: number
  col: number
  text: string
  matchStart: number
  matchEnd: number
}

type FileMatches = {
  path: string
  entry: FsEntry
  matches: Match[]
}

const MAX_FILE_SIZE = 2 * 1024 * 1024 // 2 MB cap per file
const MAX_TOTAL_MATCHES = 2000

function flattenFiles(entry: FsEntry, out: FsEntry[] = []): FsEntry[] {
  if (entry.kind === 'file') out.push(entry)
  else entry.children?.forEach((c) => flattenFiles(c, out))
  return out
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function buildRegex(query: string, opts: { caseSensitive: boolean; whole: boolean; regex: boolean }): RegExp | null {
  if (!query) return null
  try {
    const body = opts.regex ? query : escapeRegExp(query)
    const wrapped = opts.whole ? `\\b(?:${body})\\b` : body
    return new RegExp(wrapped, opts.caseSensitive ? 'g' : 'gi')
  } catch {
    return null
  }
}

function looksBinary(text: string): boolean {
  // crude: NUL byte in the first 512 chars
  const sample = text.slice(0, 512)
  for (let i = 0; i < sample.length; i++) if (sample.charCodeAt(i) === 0) return true
  return false
}

export function SearchPanel() {
  const root = useWorkspace((s) => s.root)
  const revealAt = useWorkspace((s) => s.revealAt)

  const [query, setQuery] = useState('')
  const [caseSensitive, setCaseSensitive] = useState(false)
  const [whole, setWhole] = useState(false)
  const [regex, setRegex] = useState(false)
  const [results, setResults] = useState<FileMatches[]>([])
  const [searching, setSearching] = useState(false)
  const [stats, setStats] = useState<{ files: number; matches: number; truncated: boolean }>({
    files: 0,
    matches: 0,
    truncated: false,
  })
  const tokenRef = useRef(0)

  const files = useMemo(() => (root ? flattenFiles(root) : []), [root])

  const run = useCallback(async () => {
    const re = buildRegex(query, { caseSensitive, whole, regex })
    tokenRef.current += 1
    const token = tokenRef.current
    if (!re) {
      setResults([])
      setStats({ files: 0, matches: 0, truncated: false })
      return
    }
    setSearching(true)
    const collected: FileMatches[] = []
    let totalMatches = 0
    let truncated = false

    for (const entry of files) {
      if (tokenRef.current !== token) return
      try {
        const handle = entry.handle as FileSystemFileHandle
        const file = await handle.getFile()
        if (file.size > MAX_FILE_SIZE) continue
        const text = await file.text()
        if (looksBinary(text)) continue
        const lines = text.split('\n')
        const fileMatches: Match[] = []
        for (let i = 0; i < lines.length; i++) {
          re.lastIndex = 0
          const line = lines[i]
          let m: RegExpExecArray | null
          while ((m = re.exec(line))) {
            fileMatches.push({
              path: entry.path,
              entry,
              line: i + 1,
              col: m.index + 1,
              text: line,
              matchStart: m.index,
              matchEnd: m.index + m[0].length,
            })
            totalMatches += 1
            if (m[0].length === 0) re.lastIndex += 1
            if (totalMatches >= MAX_TOTAL_MATCHES) {
              truncated = true
              break
            }
          }
          if (truncated) break
        }
        if (fileMatches.length > 0) collected.push({ path: entry.path, entry, matches: fileMatches })
      } catch {
        // ignore unreadable files
      }
      if (truncated) break
    }
    if (tokenRef.current !== token) return
    setResults(collected)
    setStats({ files: collected.length, matches: totalMatches, truncated })
    setSearching(false)
  }, [files, query, caseSensitive, whole, regex])

  // Debounce search-as-you-type.
  useEffect(() => {
    const handle = setTimeout(() => void run(), 200)
    return () => clearTimeout(handle)
  }, [run])

  const open = (m: Match) => {
    void revealAt(m.entry, m.line, m.col)
  }

  return (
    <div className="h-full flex flex-col bg-zinc-950 text-xs">
      <div className="p-2 space-y-2 border-b border-white/10">
        <div className="relative">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search workspace…"
            className="w-full pl-2 pr-2 py-1.5 bg-zinc-900 border border-white/10 rounded text-white outline-none focus:border-blue-400"
          />
        </div>
        <div className="flex items-center gap-1">
          <ToggleChip active={caseSensitive} onClick={() => setCaseSensitive((v) => !v)} title="Match case (Aa)">
            Aa
          </ToggleChip>
          <ToggleChip active={whole} onClick={() => setWhole((v) => !v)} title="Whole word">
            ab
          </ToggleChip>
          <ToggleChip active={regex} onClick={() => setRegex((v) => !v)} title="Regular expression">
            .*
          </ToggleChip>
          <div className="ml-auto text-[11px] text-zinc-500">
            {searching
              ? 'Searching…'
              : query
                ? `${stats.matches} in ${stats.files}${stats.truncated ? '+' : ''}`
                : ''}
          </div>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-auto">
        {!root && (
          <div className="p-4 text-zinc-500">Open a folder to search.</div>
        )}
        {root && results.length === 0 && query && !searching && (
          <div className="p-4 text-zinc-500">No results.</div>
        )}
        {results.map((file) => (
          <FileGroup key={file.path} group={file} onOpen={open} />
        ))}
      </div>
    </div>
  )
}

function ToggleChip({
  active,
  onClick,
  title,
  children,
}: {
  active: boolean
  onClick: () => void
  title: string
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      className={clsx(
        'h-6 px-1.5 rounded border text-[11px] font-mono',
        active
          ? 'bg-blue-500/20 border-blue-400 text-blue-200'
          : 'bg-zinc-900 border-white/10 text-zinc-400 hover:text-zinc-100',
      )}
    >
      {children}
    </button>
  )
}

function FileGroup({ group, onOpen }: { group: FileMatches; onOpen: (m: Match) => void }) {
  const [collapsed, setCollapsed] = useState(false)
  const name = group.path.split('/').pop()
  const dir = group.path.slice(0, group.path.length - (name?.length ?? 0) - 1)
  return (
    <div className="border-b border-white/5">
      <button
        type="button"
        onClick={() => setCollapsed((v) => !v)}
        className="w-full px-2 py-1 flex items-center gap-2 text-left hover:bg-white/5"
      >
        <span className="text-zinc-500">{collapsed ? '▸' : '▾'}</span>
        <span className="text-zinc-100 truncate">{name}</span>
        <span className="text-[11px] text-zinc-500 truncate">{dir}</span>
        <span className="ml-auto text-[11px] text-zinc-500">{group.matches.length}</span>
      </button>
      {!collapsed && (
        <ul>
          {group.matches.map((m, i) => (
            <li
              key={i}
              onClick={() => onOpen(m)}
              className="px-2 py-0.5 pl-7 flex gap-2 cursor-pointer hover:bg-white/5 text-zinc-400 font-mono text-[12px] leading-5"
            >
              <span className="text-zinc-600 shrink-0 w-10 text-right">{m.line}</span>
              <span className="truncate">
                <span>{m.text.slice(0, m.matchStart)}</span>
                <mark className="bg-yellow-500/30 text-yellow-100">
                  {m.text.slice(m.matchStart, m.matchEnd)}
                </mark>
                <span>{m.text.slice(m.matchEnd)}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
