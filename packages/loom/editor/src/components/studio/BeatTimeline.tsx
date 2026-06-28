// Editing-dock beat flow-DAG (IDE redesign v2 §13). A *static* story
// laid out across "time": every beat is a node, every `->` divert /
// choice / tunnel is a directed edge, auto-positioned left→right by
// reach-depth from the entry beat so a non-linear narrative reads as a
// flow map. Built from the in-browser wasm parse — no play session.
//
// Still editable: click a node to reveal that beat in the editor,
// add a beat from the toolbar, or delete one from the node's context
// menu — each rewrites `.loom` source through the parser's `edit` ops
// (the §10 "edits round-trip to source" invariant).

import { useMemo } from 'react'
import clsx from 'clsx'
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
import { openContextMenu } from '@/store/context-menu'
import { useLoomEdit, useLoomParser } from '@/lib/loom-ast'
import { beatDepths, buildStoryModel, type StoryModel } from '@/lib/loom-story'

const COL_W = 220
const ROW_GAP = 84

const EDGE_COLOR: Record<string, string> = {
  divert: '#64748b',
  choice: '#34d399',
  tunnel: '#a78bfa',
}

function layout(model: StoryModel): { nodes: Node[]; edges: Edge[] } {
  const depth = beatDepths(model)
  const order = new Map(model.beats.map((b, i) => [b.name, i]))
  const byCol = new Map<number, string[]>()
  for (const b of model.beats) {
    const d = depth.get(b.name) ?? 0
    if (!byCol.has(d)) byCol.set(d, [])
    byCol.get(d)!.push(b.name)
  }
  for (const list of byCol.values())
    list.sort((a, b) => (order.get(a) ?? 0) - (order.get(b) ?? 0))

  const pos = new Map<string, { x: number; y: number }>()
  for (const [d, list] of byCol)
    list.forEach((name, i) => pos.set(name, { x: 20 + d * COL_W, y: 20 + i * ROW_GAP }))

  const entry = model.entry
  const nodes: Node[] = model.beats.map((b) => {
    const isEntry = b.name === entry
    return {
      id: b.name,
      position: pos.get(b.name)!,
      data: {
        label: (
          <div>
            <div style={{ fontWeight: 600, color: '#c7d2fe' }}>
              {isEntry ? '▸ ' : ''}
              {b.name}
            </div>
            <div style={{ fontSize: 9, opacity: 0.65 }}>
              {b.setting ? `@ ${b.setting} · ` : ''}
              {b.items} item{b.items === 1 ? '' : 's'}
            </div>
          </div>
        ),
      },
      sourcePosition: Position.Right,
      targetPosition: Position.Left,
      style: {
        width: 168,
        background: isEntry ? '#26224d' : '#1b1830',
        border: `1px solid ${isEntry ? '#818cf8' : '#6366f155'}`,
        borderLeft: `3px solid ${isEntry ? '#818cf8' : '#6366f1'}`,
        borderRadius: 8,
        padding: '6px 8px',
        textAlign: 'left' as const,
      },
    }
  })

  const beatSet = new Set(model.beats.map((b) => b.name))
  const edges: Edge[] = []
  for (const e of model.edges) {
    if (e.from === e.to) continue
    if (!beatSet.has(e.from) || !beatSet.has(e.to)) continue
    const color = EDGE_COLOR[e.kind] ?? '#64748b'
    edges.push({
      id: `${e.from}->${e.to}:${e.kind}`,
      source: e.from,
      target: e.to,
      label: e.label && e.label.length > 24 ? `${e.label.slice(0, 23)}…` : e.label,
      labelStyle: { fill: '#94a3b8', fontSize: 9 },
      labelBgStyle: { fill: '#15191e', fillOpacity: 0.75 },
      style: { stroke: color, strokeWidth: 1.4, strokeDasharray: e.kind === 'tunnel' ? '4 3' : undefined },
      markerEnd: { type: MarkerType.ArrowClosed, color, width: 14, height: 14 },
    })
  }
  return { nodes, edges }
}

export function BeatTimeline() {
  const activePath = useWorkspace((s) => s.activePath)
  const contents = useWorkspace((s) =>
    s.activePath ? s.openFiles[s.activePath]?.contents : undefined,
  )
  const updateContents = useWorkspace((s) => s.updateContents)
  const revealActive = useWorkspace((s) => s.revealActive)
  const pin = useFocus((s) => s.pin)
  const setMode = useMode((s) => s.setMode)
  const parse = useLoomParser()
  const edit = useLoomEdit()

  const isLoom = !!activePath && activePath.endsWith('.loom')
  const { model, graph } = useMemo(() => {
    if (!parse || !isLoom || contents == null) return { model: null, graph: { nodes: [], edges: [] } }
    try {
      const m = buildStoryModel(parse(contents).ast)
      return { model: m, graph: layout(m) }
    } catch {
      return { model: null, graph: { nodes: [], edges: [] } }
    }
  }, [parse, isLoom, contents])

  if (!isLoom) return <Empty>Open a <code className="text-zinc-400">.loom</code> file to map its beats.</Empty>
  if (!parse) return <Empty>Loading parser…</Empty>
  if (!model || model.beats.length === 0) return <Empty>No beats in this file yet.</Empty>

  const lineOf = (name: string) => model.beats.find((b) => b.name === name)?.line ?? 0
  const toSource = (name: string) => {
    revealActive(lineOf(name) + 1)
    pin({ kind: 'beat', name })
    setMode('writing')
  }

  const addBeat = () => {
    if (!edit || contents == null || !activePath) return
    const name = window.prompt('New beat name')?.trim()
    if (!name) return
    try {
      const next = edit.insertBeat(contents, name, 'end', '')
      if (next !== contents) updateContents(activePath, next)
    } catch {
      /* invalid name — leave source untouched */
    }
  }

  const onRemove = (name: string) => {
    if (!edit || contents == null || !activePath) return
    if (!window.confirm(`Delete beat "${name}"? This rewrites the .loom source.`)) return
    try {
      const next = edit.removeBeat(contents, name)
      if (next !== contents) updateContents(activePath, next)
    } catch {
      /* ignore */
    }
  }

  return (
    <div className="h-full flex flex-col bg-[#15191e]">
      <div className="h-7 px-2 flex items-center text-[11px] text-zinc-400 border-b border-white/10 shrink-0">
        <span className="text-zinc-300 font-medium">Beat flow</span>
        <span className="ml-2 text-zinc-600">{model.beats.length}</span>
        <button
          type="button"
          onClick={addBeat}
          disabled={!edit}
          className={clsx(
            'ml-3 px-1.5 rounded border border-dashed border-white/20',
            edit ? 'text-zinc-300 hover:text-zinc-100 hover:border-white/40' : 'text-zinc-700',
          )}
          title="Add a beat at the end"
        >
          + beat
        </button>
        <span className="ml-auto text-zinc-600">double-click → source · right-click to edit</span>
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
          proOptions={{ hideAttribution: true }}
          onNodeClick={(_, n) => {
            revealActive(lineOf(n.id) + 1)
            pin({ kind: 'beat', name: n.id })
          }}
          onNodeDoubleClick={(_, n) => toSource(n.id)}
          onNodeContextMenu={(e, n) => {
            e.preventDefault()
            openContextMenu(
              [
                { label: 'Reveal in editor', onSelect: () => toSource(n.id) },
                { label: `Delete beat "${n.id}"`, kind: 'danger', onSelect: () => onRemove(n.id) },
              ],
              { x: e.clientX, y: e.clientY },
            )
          }}
        >
          <Background gap={18} size={1} color="#1f2430" />
          <Controls className="!bg-zinc-900 !border-white/10" showInteractive={false} />
        </ReactFlow>
      </div>
    </div>
  )
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="h-full grid place-items-center bg-[#15191e] text-zinc-500 text-xs px-4 text-center">
      <div>{children}</div>
    </div>
  )
}
