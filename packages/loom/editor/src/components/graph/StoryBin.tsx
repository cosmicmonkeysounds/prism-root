// The Story Bin — the Editing-mode project browser (Articy "navigator").
//
// Every beat and declared entity in the whole project, grouped and
// filterable, independent of which file is open. Click selects the
// node on the canvas (and pins the focus bus); double-click jumps to
// authored source in Writing mode.

import { useMemo, useState } from 'react'
import clsx from 'clsx'
import type { GraphBeat, GraphEntity } from '@loom/core/lsp'
import { pathForUri } from '@/lib/lsp-client'
import { findFileEntryByPath } from '@/lib/lsp-nav'
import { useStoryGraph } from '@/lib/story-graph'
import { useFocus } from '@/store/focus'
import { useGraph } from '@/store/graph'
import { useMode } from '@/store/mode'
import { useWorkspace } from '@/store/workspace'
import { entityGlyph } from './metrics'

const ENTITY_ORDER: Array<[string, string]> = [
  ['character', 'Characters'],
  ['role', 'Roles'],
  ['trait', 'Traits'],
  ['location', 'Locations'],
  ['faction', 'Factions'],
  ['item', 'Items'],
  ['cohort', 'Cohorts'],
  ['space', 'Spaces'],
  ['channel', 'Channels'],
  ['stats', 'Stats'],
  ['tree', 'Trees'],
  ['scene', 'Scenes'],
  ['generator', 'Generators'],
  ['person', 'People'],
  ['roster', 'Rosters'],
]

export function StoryBin() {
  const graph = useStoryGraph()
  const [filter, setFilter] = useState('')
  const selected = useGraph((s) => s.selected)
  const select = useGraph((s) => s.select)
  const openProject = useGraph((s) => s.openProject)
  const pin = useFocus((s) => s.pin)
  const setMode = useMode((s) => s.setMode)

  const q = filter.trim().toLowerCase()

  const beatsByFile = useMemo(() => {
    const match = (name: string): boolean => q.length === 0 || name.toLowerCase().includes(q)
    const out: Array<{ uri: string; path: string; beats: GraphBeat[] }> = []
    for (const f of graph.files) {
      const beats = f.beats
        .map((k) => graph.beats.get(k))
        .filter((b): b is GraphBeat => b !== undefined && match(b.key))
      if (beats.length > 0) out.push({ uri: f.uri, path: pathForUri(f.uri), beats })
    }
    return out
  }, [graph, q])

  const entityGroups = useMemo(() => {
    const match = (name: string): boolean => q.length === 0 || name.toLowerCase().includes(q)
    const byKind = new Map<string, GraphEntity[]>()
    for (const e of graph.entities.values()) {
      if (!match(e.name)) continue
      const list = byKind.get(e.kind)
      if (list) list.push(e)
      else byKind.set(e.kind, [e])
    }
    for (const list of byKind.values()) list.sort((a, b) => a.name.localeCompare(b.name))
    return byKind
  }, [graph, q])

  const revealEntity = async (e: GraphEntity): Promise<void> => {
    const ws = useWorkspace.getState()
    const entry = ws.root ? findFileEntryByPath(ws.root, pathForUri(e.uri)) : null
    if (entry) await ws.revealAt(entry, e.span.start.line + 1, e.span.start.column + 1)
    setMode('writing')
  }

  const revealBeat = async (b: GraphBeat): Promise<void> => {
    if (b.uri === null || b.span === null) return
    const ws = useWorkspace.getState()
    const entry = ws.root ? findFileEntryByPath(ws.root, pathForUri(b.uri)) : null
    if (entry) await ws.revealAt(entry, b.span.start.line + 1, b.span.start.column + 1)
    setMode('writing')
  }

  return (
    <div className="h-full flex flex-col bg-zinc-950 text-zinc-300">
      <div className="p-2 border-b border-white/10 shrink-0">
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter story bin…"
          className="w-full rounded border border-white/10 bg-zinc-900 px-2 py-1 text-[11px] placeholder:text-zinc-600 focus:outline-none focus:border-sky-400/50"
          data-testid="story-bin-filter"
        />
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto px-1 py-1.5 text-[11px]">
        <Section title={`Beats · ${graph.beats.size}`}>
          {beatsByFile.map(({ uri, path, beats }) => (
            <div key={uri} className="mb-1.5">
              <div className="px-2 py-0.5 text-[9px] uppercase tracking-wide text-zinc-600 truncate">
                {path}
              </div>
              {beats.map((b) => (
                <button
                  key={b.key}
                  className={clsx(
                    'flex w-full items-center gap-1.5 rounded px-2 py-1 text-left hover:bg-white/5',
                    selected === b.key && 'bg-sky-500/10 text-sky-200',
                  )}
                  onClick={() => {
                    openProject()
                    select(b.key)
                    pin({ kind: 'beat', name: b.name })
                  }}
                  onDoubleClick={() => void revealBeat(b)}
                  title={`${b.key} — double-click for source`}
                  data-testid={`bin-beat-${b.key}`}
                >
                  {b.entry && <span className="text-amber-300">▶</span>}
                  <span className="truncate">
                    {b.owner !== null && <span className="text-teal-300/80">{b.owner}.</span>}
                    {b.name}
                  </span>
                  {b.shadowed && <span className="text-rose-400">⚠</span>}
                  {b.structural === 'derived' && <span className="text-fuchsia-300">◈</span>}
                  <span className="ml-auto text-[9px] text-zinc-600">
                    {b.counts.choices > 0 ? `◇${b.counts.choices} ` : ''}
                    {b.counts.diverts > 0 ? `→${b.counts.diverts}` : ''}
                  </span>
                </button>
              ))}
            </div>
          ))}
          {beatsByFile.length === 0 && <Empty>no beats{q.length > 0 ? ' match' : ' yet'}</Empty>}
        </Section>

        {ENTITY_ORDER.map(([kind, label]) => {
          const list = entityGroups.get(kind)
          if (list === undefined || list.length === 0) return null
          return (
            <Section key={kind} title={`${label} · ${list.length}`}>
              {list.map((e) => {
                const { glyph, cls } = entityGlyph(e.kind)
                return (
                  <button
                    key={e.id}
                    className={clsx(
                      'flex w-full items-center gap-1.5 rounded px-2 py-1 text-left hover:bg-white/5',
                      selected === e.id && 'bg-sky-500/10 text-sky-200',
                    )}
                    onClick={() => {
                      select(e.id)
                      if (e.kind === 'character' || e.kind === 'role') {
                        pin({ kind: 'character', name: e.name })
                      }
                    }}
                    onDoubleClick={() => void revealEntity(e)}
                    title={`${e.kind} ${e.name} — double-click for source`}
                    data-testid={`bin-entity-${e.id}`}
                  >
                    <span className={clsx('text-[10px]', cls.split(' ')[0])}>{glyph}</span>
                    <span className="truncate">{e.name}</span>
                    {e.hookCount > 0 && (
                      <span className="ml-auto text-[9px] text-zinc-600">⚡{e.hookCount}</span>
                    )}
                  </button>
                )
              })}
            </Section>
          )
        })}
      </div>
    </div>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  const [open, setOpen] = useState(true)
  return (
    <div className="mb-1">
      <button
        className="flex w-full items-center gap-1 px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-zinc-500 hover:text-zinc-300"
        onClick={() => setOpen((o) => !o)}
      >
        <span className={clsx('transition-transform', open ? 'rotate-90' : '')}>▸</span>
        {title}
      </button>
      {open && children}
    </div>
  )
}

function Empty({ children }: { children: React.ReactNode }) {
  return <div className="px-2 py-1 text-zinc-600">{children}</div>
}
