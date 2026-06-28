// Editing-center entity Graph (IDE redesign v2). The full
// relationship map of a *static* `.loom` file: beats laid out left→right
// by reach-depth, with the characters / locations / cohorts they touch
// orbiting them. Built entirely from the in-browser wasm parse — no
// relay, no play session. Clicking any node reveals it in the editor.

import { useMemo } from 'react'
import {
  ReactFlow,
  Background,
  Controls,
  MarkerType,
  Position,
  type Edge,
  type Node,
} from '@xyflow/react'
import { useWorkspace } from '@/store/workspace'
import { useFocus } from '@/store/focus'
import { useMode } from '@/store/mode'
import { useLoomParser } from '@/lib/loom-ast'
import { beatDepths, buildStoryModel, type StoryModel } from '@/lib/loom-story'

const COL_W = 230
const ROW_GAP = 78
const ENTITY_GAP = 150
const BAND_GAP = 90

const KIND_STYLE: Record<string, { bg: string; border: string; text: string }> = {
  beat: { bg: '#1e1b4b', border: '#6366f1', text: '#c7d2fe' },
  character: { bg: '#0c2a33', border: '#22d3ee', text: '#a5f3fc' },
  location: { bg: '#332006', border: '#f59e0b', text: '#fcd9a0' },
  cohort: { bg: '#2a0e33', border: '#c026d3', text: '#f0abfc' },
}

const EDGE_STYLE: Record<string, { stroke: string; dash?: string }> = {
  divert: { stroke: '#64748b' },
  choice: { stroke: '#34d399' },
  tunnel: { stroke: '#a78bfa', dash: '4 3' },
  cast: { stroke: '#22d3ee', dash: '2 4' },
  setting: { stroke: '#f59e0b', dash: '2 4' },
  contains: { stroke: '#f59e0b', dash: '1 4' },
}

function nodeStyle(kind: keyof typeof KIND_STYLE, width: number) {
  const c = KIND_STYLE[kind]
  return {
    width,
    background: c.bg,
    border: `1px solid ${c.border}66`,
    borderLeft: `3px solid ${c.border}`,
    borderRadius: 8,
    color: c.text,
    fontSize: 11,
    padding: '6px 8px',
    textAlign: 'left' as const,
  }
}

function buildGraph(model: StoryModel): { nodes: Node[]; edges: Edge[] } {
  const depth = beatDepths(model)
  const order = new Map(model.beats.map((b, i) => [b.name, i]))
  const beatNames = new Set(model.beats.map((b) => b.name))

  // Beats → depth columns, stacked in source order within a column.
  const byCol = new Map<number, string[]>()
  for (const b of model.beats) {
    const d = depth.get(b.name) ?? 0
    if (!byCol.has(d)) byCol.set(d, [])
    byCol.get(d)!.push(b.name)
  }
  for (const list of byCol.values())
    list.sort((a, b) => (order.get(a) ?? 0) - (order.get(b) ?? 0))

  const beatPos = new Map<string, { x: number; y: number }>()
  let maxStack = 1
  for (const [d, list] of byCol) {
    maxStack = Math.max(maxStack, list.length)
    list.forEach((name, i) => beatPos.set(name, { x: 40 + d * COL_W, y: 120 + i * ROW_GAP }))
  }
  const beatBandBottom = 120 + maxStack * ROW_GAP

  const nodes: Node[] = []
  const edges: Edge[] = []

  for (const b of model.beats) {
    const p = beatPos.get(b.name)!
    const meta = [
      b.setting ? `@ ${b.setting}` : null,
      `${b.items} item${b.items === 1 ? '' : 's'}`,
    ]
      .filter(Boolean)
      .join('  ')
    nodes.push({
      id: `beat:${b.name}`,
      position: p,
      data: {
        label: (
          <div>
            <div style={{ fontWeight: 600 }}>{b.name}</div>
            <div style={{ fontSize: 9, opacity: 0.7 }}>{meta}</div>
          </div>
        ),
      },
      sourcePosition: Position.Right,
      targetPosition: Position.Left,
      style: nodeStyle('beat', 170),
    })
  }

  // Characters along the top band; locations + cohorts below the beats.
  const chars = model.entities.filter((e) => e.kind === 'character')
  const locs = model.entities.filter((e) => e.kind === 'location')
  const cohorts = model.entities.filter((e) => e.kind === 'cohort')

  chars.forEach((e, i) => {
    nodes.push({
      id: `character:${e.name}`,
      position: { x: 40 + i * ENTITY_GAP, y: 0 },
      data: { label: `◑ ${e.name}` },
      sourcePosition: Position.Bottom,
      targetPosition: Position.Bottom,
      style: nodeStyle('character', 120),
    })
  })
  const locY = beatBandBottom + BAND_GAP
  locs.forEach((e, i) => {
    nodes.push({
      id: `location:${e.name}`,
      position: { x: 40 + i * ENTITY_GAP, y: locY },
      data: { label: `▦ ${e.name}` },
      sourcePosition: Position.Top,
      targetPosition: Position.Top,
      style: nodeStyle('location', 120),
    })
  })
  const cohortY = locY + ENTITY_GAP - 40
  cohorts.forEach((e, i) => {
    nodes.push({
      id: `cohort:${e.name}`,
      position: { x: 40 + i * ENTITY_GAP, y: cohortY },
      data: { label: `❖ ${e.name}` },
      sourcePosition: Position.Top,
      targetPosition: Position.Top,
      style: nodeStyle('cohort', 120),
    })
  })

  const charNames = new Set(chars.map((e) => e.name))
  const locNames = new Set(locs.map((e) => e.name))

  const edge = (
    id: string,
    source: string,
    target: string,
    kind: keyof typeof EDGE_STYLE,
    label?: string,
  ): Edge => {
    const s = EDGE_STYLE[kind]
    const narrative = kind === 'divert' || kind === 'choice' || kind === 'tunnel'
    return {
      id,
      source,
      target,
      label: label && label.length > 22 ? `${label.slice(0, 21)}…` : label,
      labelStyle: { fill: '#94a3b8', fontSize: 9 },
      labelBgStyle: { fill: '#0f1115', fillOpacity: 0.7 },
      style: {
        stroke: s.stroke,
        strokeWidth: narrative ? 1.4 : 0.8,
        strokeDasharray: s.dash,
        opacity: narrative ? 0.85 : 0.4,
      },
      markerEnd: narrative
        ? { type: MarkerType.ArrowClosed, color: s.stroke, width: 14, height: 14 }
        : undefined,
    }
  }

  // Narrative transitions between beats.
  for (const e of model.edges) {
    if (!beatNames.has(e.from) || !beatNames.has(e.to)) continue
    edges.push(edge(`n:${e.from}->${e.to}:${e.kind}`, `beat:${e.from}`, `beat:${e.to}`, e.kind, e.label))
  }
  // Cast (beat → character) and setting (beat → location).
  for (const b of model.beats) {
    for (const c of b.cast)
      if (charNames.has(c)) edges.push(edge(`cast:${b.name}:${c}`, `beat:${b.name}`, `character:${c}`, 'cast'))
    if (b.setting && locNames.has(b.setting))
      edges.push(edge(`set:${b.name}`, `beat:${b.name}`, `location:${b.setting}`, 'setting'))
  }
  // Location nesting.
  for (const c of model.contains)
    if (locNames.has(c.from) && locNames.has(c.to))
      edges.push(edge(`con:${c.from}:${c.to}`, `location:${c.from}`, `location:${c.to}`, 'contains'))

  return { nodes, edges }
}

export function GraphPanel() {
  const activePath = useWorkspace((s) => s.activePath)
  const contents = useWorkspace((s) =>
    s.activePath ? s.openFiles[s.activePath]?.contents : undefined,
  )
  const revealActive = useWorkspace((s) => s.revealActive)
  const pin = useFocus((s) => s.pin)
  const setMode = useMode((s) => s.setMode)
  const parse = useLoomParser()

  const isLoom = !!activePath && activePath.endsWith('.loom')
  const { model, graph } = useMemo(() => {
    if (!parse || !isLoom || contents == null) return { model: null, graph: { nodes: [], edges: [] } }
    try {
      const m = buildStoryModel(parse(contents).ast)
      return { model: m, graph: buildGraph(m) }
    } catch {
      return { model: null, graph: { nodes: [], edges: [] } }
    }
  }, [parse, isLoom, contents])

  if (!isLoom)
    return <Empty>Open a <code className="text-zinc-400">.loom</code> file to see its story graph.</Empty>
  if (!parse) return <Empty>Loading parser…</Empty>
  if (!model || graph.nodes.length === 0) return <Empty>Nothing to graph in this file yet.</Empty>

  const onNodeClick = (line: number, kind: 'beat' | 'character' | 'location' | 'cohort', name: string) => {
    revealActive(line + 1)
    if (kind === 'beat') pin({ kind: 'beat', name })
    else if (kind === 'character') pin({ kind: 'character', name })
  }

  const lineOf = (id: string): { line: number; kind: 'beat' | 'character' | 'location' | 'cohort'; name: string } | null => {
    const [kind, name] = id.split(/:(.+)/)
    if (kind === 'beat') return { line: model.beats.find((b) => b.name === name)?.line ?? 0, kind, name }
    const ent = model.entities.find((e) => e.name === name && e.kind === kind)
    if (ent) return { line: ent.line, kind: kind as 'character' | 'location' | 'cohort', name }
    return null
  }

  return (
    <div className="h-full flex flex-col bg-[#0f1115]">
      <div className="h-7 px-2 flex items-center gap-3 text-[11px] text-zinc-400 border-b border-white/10 shrink-0">
        <span className="text-zinc-300 font-medium">Story graph</span>
        <span className="text-zinc-600">{model.beats.length} beats · {model.entities.length} entities</span>
        <span className="text-zinc-600 hidden md:inline">· double-click → source</span>
        <span className="ml-auto flex items-center gap-2 text-[10px]">
          <Legend color={KIND_STYLE.beat.border}>beat</Legend>
          <Legend color={KIND_STYLE.character.border}>character</Legend>
          <Legend color={KIND_STYLE.location.border}>location</Legend>
          <Legend color={KIND_STYLE.cohort.border}>cohort</Legend>
        </span>
      </div>
      <div className="flex-1 min-h-0">
        <ReactFlow
          nodes={graph.nodes}
          edges={graph.edges}
          fitView
          minZoom={0.1}
          colorMode="dark"
          nodesDraggable
          nodesConnectable={false}
          elementsSelectable
          proOptions={{ hideAttribution: true }}
          onNodeClick={(_, n) => {
            const info = lineOf(n.id)
            if (info) onNodeClick(info.line, info.kind, info.name)
          }}
          onNodeDoubleClick={(_, n) => {
            const info = lineOf(n.id)
            if (!info) return
            onNodeClick(info.line, info.kind, info.name)
            setMode('writing')
          }}
        >
          <Background gap={18} size={1} color="#1f2430" />
          <Controls className="!bg-zinc-900 !border-white/10" showInteractive={false} />
        </ReactFlow>
      </div>
    </div>
  )
}

function Legend({ color, children }: { color: string; children: React.ReactNode }) {
  return (
    <span className="flex items-center gap-1 text-zinc-500">
      <span className="w-2 h-2 rounded-sm" style={{ background: color }} />
      {children}
    </span>
  )
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="h-full grid place-items-center bg-[#0f1115] text-zinc-500 text-xs px-4 text-center">
      <div>{children}</div>
    </div>
  )
}
