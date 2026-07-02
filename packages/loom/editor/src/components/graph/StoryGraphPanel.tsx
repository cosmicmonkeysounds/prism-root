// The Editing-mode center stage: the global story-graph node editor.
//
// Two nested levels, Articy-style:
//   · PROJECT — every beat in every file, grouped in file containers,
//     with cross-file divert / choice / tunnel / hook edges. ELK
//     compound layout; drag positions persist per project.
//   · BEAT (drill-in, double-click) — the beat's body as a 1:1 node
//     flow (dialogue, choices fanning out, branch heads, exits).
//
// Structural edits round-trip to `.loom` source through `@loom/core`'s
// span-preserving TextEdit ops: drag-connect appends a divert, edge
// reconnect rewires a target, context menus create / rename / delete
// beats. In `run` mode the same canvas goes read-only and lights up
// with the runtime overlay (visits + current beat).

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type Connection,
  type Edge,
  type Node,
  type NodeChange,
} from '@xyflow/react'
import clsx from 'clsx'
import {
  appendDivert,
  insertBeat,
  parse,
  removeBeat,
  replaceExact,
  EditError,
} from '@loom/core/parser'
import type { GraphBeat, GraphEdge, StoryGraph } from '@loom/core/lsp'
import { docText, lspWorkspaceSync, uriFor } from '@/lib/lsp-client'
import { pathForUri } from '@/lib/lsp-client'
import { findFileEntryByPath } from '@/lib/lsp-nav'
import {
  applyEditMap,
  applyEditsToUri,
  rewireGraphEdge,
  useProjectKey,
  useStoryGraph,
  writtenTargetFor,
} from '@/lib/story-graph'
import { useFocus } from '@/store/focus'
import { useGraph } from '@/store/graph'
import { useMode } from '@/store/mode'
import { useWorkspace } from '@/store/workspace'
import { openContextMenu } from '@/store/context-menu'
import { buildBeatFlow } from './beat-flow'
import {
  BodyBranchNode,
  BodyChoiceNode,
  BodyDialogueNode,
  BodyDirectiveNode,
  BodyExitNode,
  BodySlotNode,
  BodyStartNode,
  BodyTextNode,
  type BodyExitData,
  type SourceAnchor,
} from './body-nodes'
import { buildProjectFlow } from './flow'
import { layeredLayout } from './layout'
import { BeatNode, EndNode, EntityNode, FileGroupNode, GhostNode } from './nodes'

const ALL_NODE_TYPES = {
  beat: BeatNode,
  fileGroup: FileGroupNode,
  entity: EntityNode,
  ghost: GhostNode,
  end: EndNode,
  bodyStart: BodyStartNode,
  bodyText: BodyTextNode,
  bodyDialogue: BodyDialogueNode,
  bodyDirective: BodyDirectiveNode,
  bodyChoice: BodyChoiceNode,
  bodyBranch: BodyBranchNode,
  bodyExit: BodyExitNode,
  bodySlot: BodySlotNode,
} as const

/** Drill-in node types that support in-place source editing. */
const INLINE_EDITABLE = new Set(['bodyText', 'bodyChoice', 'bodyDialogue', 'bodyDirective'])

/** The current raw source slice behind an anchor (the inline-edit seed). */
function sliceOf(anchor: NonNullable<SourceAnchor>): string | null {
  const text = docText(anchor.uri)
  if (text === null) return null
  return text.slice(anchor.span.start.offset, Math.min(anchor.span.end.offset, text.length))
}

export function StoryGraphPanel({ variant = 'edit' }: { variant?: 'edit' | 'run' }) {
  return (
    <ReactFlowProvider>
      <Canvas variant={variant} />
    </ReactFlowProvider>
  )
}

// ---------------------------------------------------------------------------
// Canvas
// ---------------------------------------------------------------------------

function Canvas({ variant }: { variant: 'edit' | 'run' }) {
  const graph = useStoryGraph()
  const view = useGraph((s) => s.view)
  const overlays = useGraph((s) => s.overlays)
  const runtime = useGraph((s) => s.runtime)
  const selected = useGraph((s) => s.selected)
  const selectedEdge = useGraph((s) => s.selectedEdge)
  const editingNode = useGraph((s) => s.editingNode)
  const centerRequest = useGraph((s) => s.centerRequest)
  const search = useGraph((s) => s.search)
  const projectKey = useProjectKey()
  const editable = variant === 'edit'

  const [positioned, setPositioned] = useState<{ nodes: Node[]; edges: Edge[] } | null>(null)
  const [status, setStatus] = useState<string | null>(null)
  const [tip, setTip] = useState<TipState | null>(null)
  const flowRef = useRef<HTMLDivElement>(null)
  const rf = useReactFlow()

  const say = useCallback((msg: string) => {
    setStatus(msg)
    window.setTimeout(() => setStatus((s) => (s === msg ? null : s)), 4000)
  }, [])

  // ---- build + layout --------------------------------------------------

  const beatKey = view.kind === 'beat' ? view.beatKey : null
  const flow = useMemo(() => {
    if (beatKey !== null) {
      const beat = graph.beats.get(beatKey)
      const body = beat !== undefined ? lspWorkspaceSync().beatBody(beatKey) : null
      if (beat === undefined || body === null) return null
      return { kind: 'beat' as const, ...buildBeatFlow(graph, beat, body) }
    }
    return { kind: 'project' as const, ...buildProjectFlow(graph, overlays) }
  }, [graph, overlays, beatKey])

  // Sizes React Flow actually measured after render — fed back into a
  // second ELK pass so boxes truly fit their text (estimates only seed
  // the first paint). One measured pass per flow build.
  const measuredRef = useRef(new Map<string, { width: number; height: number }>())
  const remeasuredRef = useRef<object | null>(null)
  const [measureTick, setMeasureTick] = useState(0)

  const runLayout = useCallback(
    (f: NonNullable<typeof flow>, sizes: Map<string, { width: number; height: number }>) => {
      let cancelled = false
      const layoutNodes = f.layoutNodes.map((n) => {
        const m = sizes.get(n.id)
        return m !== undefined && !n.isGroup ? { ...n, width: m.width, height: m.height } : n
      })
      void layeredLayout(layoutNodes, f.layoutEdges, f.kind === 'beat' ? 'DOWN' : 'RIGHT').then(
        ({ positions, groupSizes }) => {
          if (cancelled) return
          // Drag overrides are read non-reactively: applying one must not
          // re-run ELK (the drag already moved the node live on the canvas).
          const overrides =
            f.kind === 'project' ? (useGraph.getState().layouts[projectKey] ?? {}) : {}
          const nodes = f.nodes.map((n) => {
            const pos = overrides[n.id] ?? positions.get(n.id) ?? { x: 0, y: 0 }
            const size = groupSizes.get(n.id)
            return {
              ...n,
              position: pos,
              ...(size !== undefined
                ? { style: { ...n.style, width: size.width, height: size.height } }
                : {}),
            }
          })
          setPositioned({ nodes, edges: f.edges })
        },
      )
      return () => {
        cancelled = true
      }
    },
    [projectKey],
  )

  useEffect(() => {
    // A null flow (missing beat mid-edit) keeps the last layout; the
    // render below falls back to the empty state instead.
    if (flow === null) return
    measuredRef.current = new Map()
    remeasuredRef.current = null
    return runLayout(flow, new Map())
  }, [flow, runLayout])

  // Second pass: once real dimensions arrive and diverge from the
  // estimates, re-run ELK with them.
  useEffect(() => {
    if (flow === null || remeasuredRef.current === flow || measureTick === 0) return
    const measured = measuredRef.current
    const real = flow.layoutNodes.filter((n) => !n.isGroup)
    if (real.length === 0) return
    let covered = 0
    let diverges = false
    for (const n of real) {
      const m = measured.get(n.id)
      if (m === undefined) continue
      covered += 1
      if (Math.abs(m.height - n.height) > 6 || Math.abs(m.width - n.width) > 6) diverges = true
    }
    if (covered < real.length) return
    remeasuredRef.current = flow
    if (!diverges) return
    return runLayout(flow, measured)
  }, [measureTick, flow, runLayout])

  const onNodesChange = useCallback((changes: NodeChange[]) => {
    let dims = false
    for (const c of changes) {
      if (c.type === 'dimensions' && c.dimensions !== undefined) {
        measuredRef.current.set(c.id, c.dimensions)
        dims = true
      }
    }
    if (dims) setMeasureTick((t) => t + 1)
  }, [])

  // Re-fit once per view change, after the layout for that view lands.
  const viewKindKey = `${flow?.kind ?? 'empty'}:${beatKey ?? ''}`
  const lastFitRef = useRef('')
  useEffect(() => {
    if (positioned === null || lastFitRef.current === viewKindKey) return
    lastFitRef.current = viewKindKey
    const t = window.setTimeout(() => rf.fitView({ padding: 0.15, duration: 240 }), 30)
    return () => window.clearTimeout(t)
  }, [positioned, viewKindKey, rf])

  // ---- center-on-request (Story Bin / tray / search) --------------------

  const centerAttemptsRef = useRef({ token: 0, tries: 0 })
  useEffect(() => {
    if (centerRequest === null || positioned === null) return
    const { id, token } = centerRequest
    if (centerAttemptsRef.current.token !== token) {
      centerAttemptsRef.current = { token, tries: 0 }
    }
    const hit = positioned.nodes.find((n) => n.id === id)
    if (hit !== undefined) {
      const abs = absolutePosition(hit, positioned.nodes)
      const w = (hit.measured?.width ?? hit.width ?? 180) as number
      const h = (hit.measured?.height ?? hit.height ?? 60) as number
      rf.setCenter(abs.x + w / 2, abs.y + h / 2, { zoom: Math.max(rf.getZoom(), 0.9), duration: 320 })
      useGraph.getState().clearCenter(token)
      return
    }
    // Not on the canvas yet. An entity hidden by the overlay? Turn the
    // overlay on and let the relayout retry this same request.
    centerAttemptsRef.current.tries += 1
    if (centerAttemptsRef.current.tries > 6) {
      useGraph.getState().clearCenter(token)
      return
    }
    if (graph.entities.has(id) && !useGraph.getState().overlays.entities) {
      useGraph.getState().setOverlay('entities', true)
      return
    }
    // A beat hidden inside a drill-in view → pop back to the project map.
    if (graph.beats.has(id) && useGraph.getState().view.kind === 'beat' && id !== beatKey) {
      useGraph.getState().openProject()
    }
  }, [centerRequest, positioned, graph, rf, beatKey])

  // ---- decoration (selection + runtime + inline editing) ----------------

  const setEditing = useGraph((s) => s.setEditing)

  const commitInlineEdit = useCallback(
    async (anchor: NonNullable<SourceAnchor>, next: string): Promise<void> => {
      const text = docText(anchor.uri)
      if (text === null) return
      const start = anchor.span.start.offset
      const end = Math.min(anchor.span.end.offset, text.length)
      try {
        const current = text.slice(start, end)
        const edits = replaceExact(text, start, end, current, next)
        if (edits.length > 0) await applyEditsToUri(anchor.uri, edits)
      } catch (e) {
        say(e instanceof EditError ? e.message : 'Edit failed — source changed underneath.')
      } finally {
        useGraph.getState().setEditing(null)
      }
    },
    [say],
  )

  const beatEditable =
    editable && beatKey !== null && graph.beats.get(beatKey)?.structural === 'file'

  const nodes = useMemo(() => {
    if (positioned === null) return []
    return positioned.nodes.map((n) => {
      const isBeat = n.type === 'beat'
      const anchor = (n.data as { anchor?: SourceAnchor }).anchor ?? null
      const inlineEditable =
        beatEditable && anchor !== null && INLINE_EDITABLE.has(n.type ?? '')
      const isEditing = inlineEditable && editingNode === n.id
      const decorated: Node = {
        ...n,
        selected: n.id === selected,
        data: {
          ...n.data,
          ...(isBeat
            ? { visits: runtime.visits[n.id], isCurrent: runtime.current === n.id }
            : {}),
          ...(inlineEditable
            ? {
                editing: isEditing,
                editSlice: isEditing && anchor !== null ? sliceOf(anchor) : null,
                onCommitEdit: (next: string) => void commitInlineEdit(anchor!, next),
                onCancelEdit: () => setEditing(null),
              }
            : {}),
        },
        draggable: editable || n.type === 'fileGroup' ? editable : false,
        connectable: editable && isBeat,
      }
      return decorated
    })
  }, [positioned, selected, runtime, editable, beatEditable, editingNode, commitInlineEdit, setEditing])

  const edges = useMemo(() => {
    if (positioned === null) return []
    return positioned.edges.map((e) => {
      if (e.id === selectedEdge) {
        return { ...e, selected: true, style: { ...e.style, strokeWidth: 2.6, opacity: 1 } }
      }
      // A selected node emphasises its own connections and dims the rest
      // (Articy-style connection highlighting).
      if (selected !== null && selectedEdge === null) {
        const touches = e.source === selected || e.target === selected
        const base = typeof e.style?.opacity === 'number' ? e.style.opacity : 0.9
        const width = typeof e.style?.strokeWidth === 'number' ? e.style.strokeWidth : 1.5
        return {
          ...e,
          selected: false,
          style: touches
            ? { ...e.style, opacity: 1, strokeWidth: width + 0.8 }
            : { ...e.style, opacity: base * 0.22 },
        }
      }
      return { ...e, selected: false }
    })
  }, [positioned, selectedEdge, selected])

  // ---- interactions -----------------------------------------------------

  const pin = useFocus((s) => s.pin)
  const openBeat = useGraph((s) => s.openBeat)
  const select = useGraph((s) => s.select)
  const moveNode = useGraph((s) => s.moveNode)
  const setMode = useMode((s) => s.setMode)

  const revealAnchor = useCallback(
    async (anchor: SourceAnchor, toWriting = true): Promise<void> => {
      if (anchor === null) return
      const path = pathForUri(anchor.uri)
      const ws = useWorkspace.getState()
      const entry = ws.root ? findFileEntryByPath(ws.root, path) : null
      if (entry) await ws.revealAt(entry, anchor.span.start.line + 1, anchor.span.start.column + 1)
      if (toWriting) setMode('writing')
    },
    [setMode],
  )

  const revealBeatSource = useCallback(
    (beat: GraphBeat, toWriting = true) => {
      if (beat.uri === null || beat.span === null) return
      void revealAnchor({ uri: beat.uri, span: beat.span }, toWriting)
    },
    [revealAnchor],
  )

  const onNodeClick = useCallback(
    (_: unknown, node: Node) => {
      select(node.id)
      const beat = graph.beats.get(node.id)
      if (beat !== undefined) pin({ kind: 'beat', name: beat.name })
      else if (node.type === 'entity') {
        const ent = graph.entities.get(node.id)
        if (ent !== undefined && (ent.kind === 'character' || ent.kind === 'role')) {
          pin({ kind: 'character', name: ent.name })
        }
      }
    },
    [graph, pin, select],
  )

  const onNodeDoubleClick = useCallback(
    (_: unknown, node: Node) => {
      // Project level: drill into a beat. Beat level: inline-edit text
      // nodes, follow exits, or fall back to opening the source.
      const beat = graph.beats.get(node.id)
      if (beat !== undefined) {
        openBeat(beat.key)
        return
      }
      if (node.type === 'bodyExit') {
        const data = node.data as BodyExitData
        if (data.resolved !== null) {
          openBeat(data.resolved)
          return
        }
      }
      // Double-clicking a ghost creates the missing beat (when nameable).
      if (node.type === 'ghost' && editable) {
        const target = (node.data as { target: string }).target
        if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(target)) void createBeat(graph, say, target)
        return
      }
      // Double-clicking a file container opens the file in Writing mode.
      if (node.type === 'fileGroup') {
        const path = (node.data as { path: string }).path
        const ws = useWorkspace.getState()
        const entry = ws.root ? findFileEntryByPath(ws.root, path) : null
        if (entry) {
          void ws.revealAt(entry, 1, 1).then(() => setMode('writing'))
        }
        return
      }
      const anchor = (node.data as { anchor?: SourceAnchor }).anchor ?? null
      if (beatEditable && anchor !== null && INLINE_EDITABLE.has(node.type ?? '')) {
        setTip(null)
        setEditing(node.id)
        return
      }
      if (anchor !== null) void revealAnchor(anchor)
      else if (node.type === 'entity') {
        const ent = graph.entities.get(node.id)
        if (ent !== undefined) void revealAnchor({ uri: ent.uri, span: ent.span })
      }
    },
    [graph, openBeat, revealAnchor, beatEditable, setEditing, editable, say, setMode],
  )

  // ---- edge selection + hover tooltips -----------------------------------

  const selectEdge = useGraph((s) => s.selectEdge)

  const onEdgeClick = useCallback(
    (_: unknown, edge: Edge) => {
      selectEdge(edge.id)
    },
    [selectEdge],
  )

  const tipTimer = useRef<number | null>(null)
  const showTip = useCallback((event: React.MouseEvent, lines: TipLine[]) => {
    if (tipTimer.current !== null) window.clearTimeout(tipTimer.current)
    const host = flowRef.current?.getBoundingClientRect()
    if (host === undefined || lines.length === 0) return
    const x = event.clientX - host.left
    const y = event.clientY - host.top
    tipTimer.current = window.setTimeout(() => setTip({ x, y, lines }), 220)
  }, [])
  const hideTip = useCallback(() => {
    if (tipTimer.current !== null) window.clearTimeout(tipTimer.current)
    tipTimer.current = null
    setTip(null)
  }, [])

  const setHover = useFocus((s) => s.setHover)

  const onNodeMouseEnter = useCallback(
    (event: React.MouseEvent, node: Node) => {
      // Feed the cross-panel focus bus (References dim/highlight, …).
      const beat = graph.beats.get(node.id)
      if (beat !== undefined) setHover({ kind: 'beat', name: beat.name })
      else {
        const ent = graph.entities.get(node.id)
        if (ent !== undefined && (ent.kind === 'character' || ent.kind === 'role')) {
          setHover({ kind: 'character', name: ent.name })
        }
      }
      if (editingNode !== null) return
      showTip(event, nodeTipLines(graph, node))
    },
    [graph, showTip, editingNode, setHover],
  )
  const onNodeMouseLeave = useCallback(() => {
    setHover(null)
    hideTip()
  }, [setHover, hideTip])
  const onEdgeMouseEnter = useCallback(
    (event: React.MouseEvent, edge: Edge) => {
      const ge = (edge.data as { graphEdge?: GraphEdge } | undefined)?.graphEdge
      showTip(event, ge !== undefined ? edgeTipLines(ge) : [])
    },
    [showTip],
  )

  // ---- keyboard flow ------------------------------------------------------
  // Esc backs out of the drill-in (or clears selection); F2 renames the
  // selected beat; Delete removes it. Never while typing somewhere.

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null
      if (
        t !== null &&
        (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t.isContentEditable)
      ) {
        return
      }
      const st = useGraph.getState()
      if (e.key === 'Escape') {
        if (st.editingNode !== null) return // the textarea owns its Esc
        if (st.view.kind === 'beat') {
          e.preventDefault()
          st.openProject()
        } else if (st.selected !== null || st.selectedEdge !== null) {
          st.select(null)
          st.selectEdge(null)
        }
        return
      }
      if (!editable || st.selected === null) return
      const beat = graph.beats.get(st.selected)
      if (beat === undefined) return
      if (e.key === 'F2') {
        e.preventDefault()
        if (beat.structural !== 'derived') void renameBeat(beat, say)
      } else if (e.key === 'Delete' || e.key === 'Backspace') {
        e.preventDefault()
        if (beat.structural === 'file') void deleteBeat(beat, say)
        else say('Only top-level beats delete from the canvas — owned/derived beats live in their owner.')
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [editable, graph, say])

  const onNodeDragStop = useCallback(
    (_: unknown, node: Node) => {
      if (view.kind !== 'project') return
      moveNode(projectKey, node.id, node.position)
    },
    [moveNode, projectKey, view.kind],
  )

  const onNodeContextMenu = useCallback(
    (event: React.MouseEvent, node: Node) => {
      event.preventDefault()
      const beat = graph.beats.get(node.id)
      const anchor = { x: event.clientX, y: event.clientY }
      if (beat !== undefined) {
        openContextMenu(
          [
            { label: 'Open beat', onSelect: () => openBeat(beat.key) },
            { label: 'Show source', onSelect: () => revealBeatSource(beat) },
            ...(editable
              ? [
                  {
                    label: 'Rename…',
                    disabled: beat.structural === 'derived',
                    onSelect: () => void renameBeat(beat, say),
                  },
                  {
                    label: 'Delete beat',
                    kind: 'danger' as const,
                    disabled: beat.structural !== 'file',
                    onSelect: () => void deleteBeat(beat, say),
                  },
                ]
              : []),
          ],
          anchor,
        )
        return
      }
      const ent = graph.entities.get(node.id)
      if (ent !== undefined) {
        openContextMenu(
          [{ label: 'Show source', onSelect: () => void revealAnchor({ uri: ent.uri, span: ent.span }) }],
          anchor,
        )
        return
      }
      // Ghost node: offer to create the missing beat — every dangling
      // divert to this name resolves the moment it exists.
      if (node.type === 'ghost' && editable) {
        const target = (node.data as { target: string }).target
        const bare = /^[A-Za-z_][A-Za-z0-9_]*$/.test(target)
        openContextMenu(
          [
            {
              label: `Create beat \`${target}\``,
              disabled: !bare,
              onSelect: () => void createBeat(graph, say, target),
            },
          ],
          anchor,
        )
        return
      }
      // Beat-view body nodes: jump to source or edit in place.
      const bodyAnchor = (node.data as { anchor?: SourceAnchor }).anchor ?? null
      if (bodyAnchor !== null) {
        const items = [
          { label: 'Show source', onSelect: () => void revealAnchor(bodyAnchor) },
        ]
        if (beatEditable && INLINE_EDITABLE.has(node.type ?? '')) {
          items.push({
            label: 'Edit in place',
            onSelect: () => {
              setEditing(node.id)
            },
          })
        }
        openContextMenu(items, anchor)
      }
    },
    [graph, editable, beatEditable, openBeat, revealBeatSource, revealAnchor, say, setEditing],
  )

  const onPaneContextMenu = useCallback(
    (event: React.MouseEvent | MouseEvent) => {
      event.preventDefault()
      if (!editable || view.kind !== 'project') return
      openContextMenu(
        [
          { label: 'New beat…', onSelect: () => void createBeat(graph, say) },
          { label: 'Reset layout', onSelect: () => useGraph.getState().resetLayout(projectKey) },
        ],
        { x: (event as MouseEvent).clientX, y: (event as MouseEvent).clientY },
      )
    },
    [editable, view.kind, graph, projectKey, say],
  )

  const onConnect = useCallback(
    (conn: Connection) => {
      if (!editable) return
      void connectBeats(graph, conn.source, conn.target, say)
    },
    [editable, graph, say],
  )

  const onReconnect = useCallback(
    (oldEdge: Edge, conn: Connection) => {
      if (!editable) return
      const ge = (oldEdge.data as { graphEdge?: GraphEdge } | undefined)?.graphEdge
      if (ge === undefined) return
      void rewireEdge(graph, ge, conn.target, say)
    },
    [editable, graph, say],
  )

  // ---- search ------------------------------------------------------------

  const onSearchSubmit = useCallback(() => {
    const q = search.trim().toLowerCase()
    if (q.length === 0) return
    // Beats first, then entities — routed through the shared reveal
    // mechanism so hidden targets pull their overlay on.
    const beat = [...graph.beats.keys()].find((k) => k.toLowerCase().includes(q))
    const ent = beat === undefined ? [...graph.entities.keys()].find((k) => k.toLowerCase().includes(q)) : undefined
    const hit = beat ?? ent
    if (hit !== undefined) useGraph.getState().reveal(hit)
  }, [search, graph])

  // ---- render ------------------------------------------------------------

  const empty = graph.beats.size === 0

  return (
    <div className="h-full flex flex-col bg-[#0f1115]" ref={flowRef}>
      <Toolbar
        variant={variant}
        graph={graph}
        onSearchSubmit={onSearchSubmit}
        status={status}
        say={say}
      />
      <div className="flex-1 min-h-0">
        {empty ? (
          <EmptyState editable={editable} say={say} graph={graph} />
        ) : flow === null ? (
          <div className="h-full grid place-items-center text-zinc-500 text-xs">
            <div className="text-center space-y-2">
              <div>This beat no longer exists.</div>
              <button
                className="rounded border border-white/10 px-3 py-1 text-zinc-300 hover:bg-white/5"
                onClick={() => useGraph.getState().openProject()}
              >
                ← Back to story map
              </button>
            </div>
          </div>
        ) : (
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={ALL_NODE_TYPES}
            minZoom={0.05}
            colorMode="dark"
            fitView
            nodesDraggable={editable && view.kind === 'project'}
            nodesConnectable={editable && view.kind === 'project'}
            edgesReconnectable={editable && view.kind === 'project'}
            elementsSelectable
            proOptions={{ hideAttribution: true }}
            onNodesChange={onNodesChange}
            onNodeClick={onNodeClick}
            onNodeDoubleClick={onNodeDoubleClick}
            onNodeDragStart={hideTip}
            onNodeDragStop={onNodeDragStop}
            onNodeContextMenu={onNodeContextMenu}
            onNodeMouseEnter={onNodeMouseEnter}
            onNodeMouseLeave={onNodeMouseLeave}
            onEdgeClick={onEdgeClick}
            onEdgeMouseEnter={onEdgeMouseEnter}
            onEdgeMouseLeave={hideTip}
            onPaneContextMenu={onPaneContextMenu}
            onPaneClick={() => {
              select(null)
              selectEdge(null)
              hideTip()
            }}
            onMoveStart={hideTip}
            onConnect={onConnect}
            onReconnect={onReconnect}
            deleteKeyCode={null}
          >
            <Background gap={18} size={1} color="#1f2430" />
            <Controls className="!bg-zinc-900 !border-white/10" showInteractive={false} />
            {view.kind === 'project' && (
              <MiniMap
                pannable
                zoomable
                className="!bg-zinc-900/90 !border !border-white/10 !rounded-md"
                maskColor="rgba(15, 17, 21, 0.78)"
                nodeStrokeWidth={2}
                nodeColor={miniMapColor}
              />
            )}
          </ReactFlow>
        )}
      </div>
      {tip !== null && <Tooltip tip={tip} />}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Hover tooltips
// ---------------------------------------------------------------------------

type TipLine = { text: string; dim?: boolean; mono?: boolean }
type TipState = { x: number; y: number; lines: TipLine[] }

function Tooltip({ tip }: { tip: TipState }) {
  return (
    <div
      className="pointer-events-none absolute z-50 max-w-80 rounded-md border border-white/15 bg-zinc-900/95 px-2.5 py-1.5 shadow-xl"
      style={{ left: tip.x + 14, top: tip.y + 10 }}
    >
      {tip.lines.map((l, i) => (
        <div
          key={i}
          className={clsx(
            'text-[10px] leading-[15px]',
            l.dim === true ? 'text-zinc-500' : 'text-zinc-200',
            l.mono === true && 'font-mono',
          )}
        >
          {l.text}
        </div>
      ))}
    </div>
  )
}

/** Tooltip content for any canvas node. */
function nodeTipLines(graph: StoryGraph, node: Node): TipLine[] {
  const beat = graph.beats.get(node.id)
  if (beat !== undefined) {
    const lines: TipLine[] = [{ text: beat.key, mono: true }]
    lines.push({
      text: `${beat.structural} beat${beat.entry ? ' · entry' : ''}${beat.shadowed ? ' · SHADOWED' : ''}`,
      dim: true,
    })
    if (beat.uri !== null && beat.span !== null) {
      lines.push({ text: `${pathForUri(beat.uri)}:${beat.span.start.line + 1}`, dim: true, mono: true })
    }
    for (const p of beat.preview) lines.push({ text: p })
    const stats: string[] = []
    if (beat.counts.dialogues > 0) stats.push(`${beat.counts.dialogues} lines`)
    if (beat.counts.choices > 0) stats.push(`${beat.counts.choices} choices`)
    if (beat.counts.diverts > 0) stats.push(`${beat.counts.diverts} diverts`)
    if (stats.length > 0) lines.push({ text: stats.join(' · '), dim: true })
    lines.push({ text: 'double-click to open · right-click for actions', dim: true })
    return lines
  }
  const ent = graph.entities.get(node.id)
  if (ent !== undefined) {
    const lines: TipLine[] = [{ text: `${ent.kind} ${ent.name}`, mono: true }]
    if (ent.faction !== null) lines.push({ text: `faction: ${ent.faction}` })
    if (ent.mixins.length > 0) lines.push({ text: `is ${ent.mixins.join(', ')}` })
    if (ent.hookCount > 0) lines.push({ text: `${ent.hookCount} reactive hook${ent.hookCount === 1 ? '' : 's'}` })
    if (ent.ownedBeats.length > 0) lines.push({ text: `owns ${ent.ownedBeats.join(', ')}`, dim: true })
    lines.push({ text: `${pathForUri(ent.uri)}:${ent.span.start.line + 1}`, dim: true, mono: true })
    return lines
  }
  if (node.type === 'ghost') {
    const target = (node.data as { target: string }).target
    return [
      { text: `-> ${target}`, mono: true },
      { text: 'no beat with this name — the divert is dangling', dim: true },
      { text: 'double-click to create it · or rewire the edge', dim: true },
    ]
  }
  // Drill-in body nodes: show anchor + a fuller text.
  const anchor = (node.data as { anchor?: SourceAnchor }).anchor ?? null
  const lines: TipLine[] = []
  const d = node.data as Record<string, unknown>
  if (typeof d.text === 'string') lines.push({ text: d.text as string })
  else if (typeof d.raw === 'string') lines.push({ text: d.raw as string, mono: true })
  else if (Array.isArray(d.lines) && typeof d.speaker === 'string') {
    lines.push({ text: d.speaker as string, mono: true })
    for (const l of d.lines as string[]) lines.push({ text: l })
  }
  if (anchor !== null) {
    lines.push({ text: `${pathForUri(anchor.uri)}:${anchor.span.start.line + 1}`, dim: true, mono: true })
    lines.push({ text: 'double-click to edit in place', dim: true })
  }
  return lines
}

/** Tooltip content for an edge. */
function edgeTipLines(e: GraphEdge): TipLine[] {
  const lines: TipLine[] = []
  const head =
    e.kind === 'hook'
      ? `${e.label ?? 'hook'}`
      : e.kind === 'choice'
        ? `choice · ${e.sticky === true ? 'sticky (+)' : 'once (*)'}`
        : e.kind
  lines.push({ text: head, mono: true })
  if (e.kind === 'choice' && e.label !== null) lines.push({ text: `“${e.label}”` })
  if (e.condition !== null) lines.push({ text: `when ${e.condition}` })
  lines.push({ text: `${e.from} → ${e.to ?? `⚠ ${e.unresolved ?? '?'}`}`, mono: true })
  if (e.dynamic) lines.push({ text: 'self-binding resolved at play time (best-effort here)', dim: true })
  if (e.uri !== null && e.span !== null) {
    lines.push({ text: `${pathForUri(e.uri)}:${e.span.start.line + 1}`, dim: true, mono: true })
  }
  lines.push({ text: 'click for details in Properties', dim: true })
  return lines
}

/** MiniMap swatches per node type. */
function miniMapColor(node: Node): string {
  switch (node.type) {
    case 'beat':
      return '#6366f1'
    case 'entity':
      return '#155e75'
    case 'ghost':
      return '#b91c1c'
    case 'end':
      return '#52525b'
    case 'fileGroup':
      return 'rgba(63, 63, 70, 0.35)'
    default:
      return '#3f3f46'
  }
}

/** Absolute canvas position (child coordinates are parent-relative). */
function absolutePosition(node: Node, all: Node[]): { x: number; y: number } {
  let x = node.position.x
  let y = node.position.y
  let parentId = node.parentId
  while (parentId !== undefined) {
    const parent = all.find((n) => n.id === parentId)
    if (parent === undefined) break
    x += parent.position.x
    y += parent.position.y
    parentId = parent.parentId
  }
  return { x, y }
}

// ---------------------------------------------------------------------------
// Toolbar (breadcrumbs · overlays · search · status)
// ---------------------------------------------------------------------------

function Toolbar({
  variant,
  graph,
  onSearchSubmit,
  status,
  say,
}: {
  variant: 'edit' | 'run'
  graph: StoryGraph
  onSearchSubmit: () => void
  status: string | null
  say: (msg: string) => void
}) {
  const view = useGraph((s) => s.view)
  const overlays = useGraph((s) => s.overlays)
  const setOverlay = useGraph((s) => s.setOverlay)
  const openProject = useGraph((s) => s.openProject)
  const search = useGraph((s) => s.search)
  const setSearch = useGraph((s) => s.setSearch)

  return (
    <div className="h-8 px-2 flex items-center gap-2 text-[11px] text-zinc-400 border-b border-white/10 shrink-0">
      <button
        className={clsx(
          'font-medium',
          view.kind === 'project' ? 'text-zinc-200' : 'text-sky-300 hover:underline',
        )}
        onClick={openProject}
        data-testid="graph-crumb-project"
      >
        Story map
      </button>
      {view.kind === 'beat' && (
        <>
          <span className="text-zinc-600">›</span>
          <span className="font-medium text-zinc-200" data-testid="graph-crumb-beat">
            {view.beatKey}
          </span>
        </>
      )}
      <span className="text-zinc-600">
        {graph.beats.size} beats · {graph.edges.filter((e) => e.narrative).length} links
      </span>
      {status !== null && <span className="text-amber-300 truncate">{status}</span>}
      <span className="ml-auto flex items-center gap-1.5">
        {view.kind === 'project' && (
          <>
            <OverlayToggle label="hooks" on={overlays.hooks} onToggle={(v) => setOverlay('hooks', v)} />
            <OverlayToggle
              label="entities"
              on={overlays.entities}
              onToggle={(v) => setOverlay('entities', v)}
            />
            <OverlayToggle label="labels" on={overlays.labels} onToggle={(v) => setOverlay('labels', v)} />
          </>
        )}
        {variant === 'edit' && view.kind === 'project' && (
          <button
            className="rounded border border-white/10 px-2 py-0.5 text-zinc-300 hover:bg-white/5"
            onClick={() => void createBeat(graph, say)}
            data-testid="graph-new-beat"
          >
            + beat
          </button>
        )}
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') onSearchSubmit()
          }}
          placeholder="find beat…"
          className="w-28 rounded border border-white/10 bg-zinc-900 px-2 py-0.5 text-[11px] text-zinc-200 placeholder:text-zinc-600 focus:outline-none focus:border-sky-400/50"
          data-testid="graph-search"
        />
      </span>
    </div>
  )
}

function OverlayToggle({
  label,
  on,
  onToggle,
}: {
  label: string
  on: boolean
  onToggle: (v: boolean) => void
}) {
  return (
    <button
      className={clsx(
        'rounded px-1.5 py-0.5 border',
        on ? 'border-sky-400/40 bg-sky-500/10 text-sky-200' : 'border-white/10 text-zinc-500 hover:text-zinc-300',
      )}
      onClick={() => onToggle(!on)}
      data-testid={`graph-overlay-${label}`}
    >
      {label}
    </button>
  )
}

function EmptyState({
  editable,
  say,
  graph,
}: {
  editable: boolean
  say: (msg: string) => void
  graph: StoryGraph
}) {
  return (
    <div className="h-full grid place-items-center text-zinc-500 text-xs">
      <div className="text-center space-y-2">
        <div>No beats in this project yet.</div>
        {editable && (
          <button
            className="rounded border border-white/10 px-3 py-1 text-zinc-300 hover:bg-white/5"
            onClick={() => void createBeat(graph, say)}
          >
            + Create the first beat
          </button>
        )}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Structural edits (round-trip to .loom source)
// ---------------------------------------------------------------------------

function editableSourceBeat(graph: StoryGraph, id: string, say: (m: string) => void): GraphBeat | null {
  const beat = graph.beats.get(id)
  if (beat === undefined) {
    say('Connections start from a beat.')
    return null
  }
  if (beat.structural !== 'file') {
    say(
      beat.structural === 'owned'
        ? 'Owned beats are edited in their owner’s source — open it with a double-click.'
        : 'Derived beats are template instances — edit the trait’s beat.',
    )
    return null
  }
  if (beat.uri === null) return null
  return beat
}

/** Drag-a-connection: append `-> target` to the source beat. */
async function connectBeats(
  graph: StoryGraph,
  sourceId: string | null,
  targetId: string | null,
  say: (m: string) => void,
): Promise<void> {
  if (sourceId === null || targetId === null) return
  const source = editableSourceBeat(graph, sourceId, say)
  if (source === null) return
  const target = graph.beats.get(targetId)
  if (target === undefined) {
    say('Connections land on beats.')
    return
  }
  const written = writtenTargetFor(target)
  if (written === null) {
    say('That beat is shadowed by a same-named beat — rename one first.')
    return
  }
  try {
    const uri = source.uri!
    const text = docText(uri)
    if (text === null) return
    const [file] = parse(text)
    const edits = appendDivert(text, file, source.name, written)
    await applyEditsToUri(uri, edits)
    say(`Connected ${source.key} → ${written}`)
  } catch (e) {
    say(e instanceof EditError ? e.message : 'Could not append the divert.')
  }
}

/** Drag an edge end onto a new beat: rewrite the divert's target text. */
async function rewireEdge(
  graph: StoryGraph,
  edge: GraphEdge,
  targetId: string | null,
  say: (m: string) => void,
): Promise<void> {
  if (targetId === null) return
  const err = await rewireGraphEdge(graph, edge, targetId)
  say(err ?? `Rewired → ${targetId}`)
}

/** Create a new beat in the active (or first) `.loom` file. */
async function createBeat(
  graph: StoryGraph,
  say: (m: string) => void,
  presetName?: string,
): Promise<void> {
  const name = presetName ?? window.prompt('New beat name:')
  if (name === null || name.trim().length === 0) return
  const clean = name.trim()
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(clean)) {
    say('Beat names are identifiers (letters, digits, underscore).')
    return
  }
  if (graph.beats.has(clean)) {
    say(`A beat named \`${clean}\` already exists.`)
    return
  }
  const ws = useWorkspace.getState()
  const active = ws.activePath !== null && ws.activePath.endsWith('.loom') ? ws.activePath : null
  const fallback = graph.files[0] !== undefined ? pathForUri(graph.files[0].uri) : null
  const path = active ?? fallback
  if (path === null) {
    say('Open a .loom file first.')
    return
  }
  const uri = uriFor(path)
  const text = docText(uri)
  if (text === null) {
    say(`\`${path}\` isn’t indexed yet.`)
    return
  }
  const [file] = parse(text)
  const edits = insertBeat(text, file, clean, { kind: 'end' })
  await applyEditsToUri(uri, edits)
  useGraph.getState().reveal(clean)
  say(`Created \`${clean}\` in ${path}`)
}

/** Rename via the workspace-wide rename (declaration + references + entry). */
async function renameBeat(beat: GraphBeat, say: (m: string) => void): Promise<void> {
  const next = window.prompt(`Rename \`${beat.key}\` to:`, beat.name)
  if (next === null || next.trim().length === 0 || next.trim() === beat.name) return
  try {
    const edits = lspWorkspaceSync().renameBeat(beat.key, next.trim())
    await applyEditMap(edits)
    const newKey = beat.owner === null ? next.trim() : `${beat.owner}.${next.trim()}`
    useGraph.getState().select(newKey)
    say(`Renamed to \`${newKey}\` (${edits.size} file${edits.size === 1 ? '' : 's'})`)
  } catch (e) {
    say(e instanceof EditError ? e.message : 'Rename failed.')
  }
}

/** Delete a top-level beat (its whole block). */
async function deleteBeat(beat: GraphBeat, say: (m: string) => void): Promise<void> {
  if (beat.uri === null) return
  const ok = window.confirm(`Delete beat \`${beat.key}\` and its contents?`)
  if (!ok) return
  const text = docText(beat.uri)
  if (text === null) return
  const [file] = parse(text)
  try {
    await applyEditsToUri(beat.uri, removeBeat(text, file, beat.name))
    useGraph.getState().select(null)
    say(`Deleted \`${beat.key}\``)
  } catch (e) {
    say(e instanceof EditError ? e.message : 'Delete failed.')
  }
}
