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
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type Connection,
  type Edge,
  type Node,
} from '@xyflow/react'
import clsx from 'clsx'
import {
  appendDivert,
  insertBeat,
  parse,
  removeBeat,
  replaceExact,
  retargetDivert,
  EditError,
  type TextEdit,
} from '@loom/core/parser'
import type { GraphBeat, GraphEdge, StoryGraph } from '@loom/core/lsp'
import { docText, lspWorkspaceSync, uriFor } from '@/lib/lsp-client'
import { pathForUri } from '@/lib/lsp-client'
import { findFileEntryByPath } from '@/lib/lsp-nav'
import { applyEditMap, applyEditsToUri, useProjectKey, useStoryGraph, writtenTargetFor } from '@/lib/story-graph'
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
  const search = useGraph((s) => s.search)
  const projectKey = useProjectKey()
  const editable = variant === 'edit'

  const [positioned, setPositioned] = useState<{ nodes: Node[]; edges: Edge[] } | null>(null)
  const [status, setStatus] = useState<string | null>(null)
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

  useEffect(() => {
    // A null flow (missing beat mid-edit) keeps the last layout; the
    // render below falls back to the empty state instead.
    if (flow === null) return
    let cancelled = false
    void layeredLayout(flow.layoutNodes, flow.layoutEdges, flow.kind === 'beat' ? 'DOWN' : 'RIGHT').then(
      ({ positions, groupSizes }) => {
        if (cancelled) return
        // Drag overrides are read non-reactively: applying one must not
        // re-run ELK (the drag already moved the node live on the canvas).
        const overrides =
          flow.kind === 'project' ? (useGraph.getState().layouts[projectKey] ?? {}) : {}
        const nodes = flow.nodes.map((n) => {
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
        setPositioned({ nodes, edges: flow.edges })
      },
    )
    return () => {
      cancelled = true
    }
  }, [flow, projectKey])

  // Re-fit once per view change, after the layout for that view lands.
  const viewKindKey = `${flow?.kind ?? 'empty'}:${beatKey ?? ''}`
  const lastFitRef = useRef('')
  useEffect(() => {
    if (positioned === null || lastFitRef.current === viewKindKey) return
    lastFitRef.current = viewKindKey
    const t = window.setTimeout(() => rf.fitView({ padding: 0.15, duration: 240 }), 30)
    return () => window.clearTimeout(t)
  }, [positioned, viewKindKey, rf])

  // ---- decoration (selection + runtime) --------------------------------

  const nodes = useMemo(() => {
    if (positioned === null) return []
    return positioned.nodes.map((n) => {
      const isBeat = n.type === 'beat'
      const decorated: Node = {
        ...n,
        selected: n.id === selected,
        ...(isBeat
          ? {
              data: {
                ...n.data,
                visits: runtime.visits[n.id],
                isCurrent: runtime.current === n.id,
              },
            }
          : {}),
        draggable: editable || n.type === 'fileGroup' ? editable : false,
        connectable: editable && isBeat,
      }
      return decorated
    })
  }, [positioned, selected, runtime, editable])

  const edges = positioned?.edges ?? []

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
      // Project level: drill into a beat. Beat level: follow exits / open source.
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
      const anchor = (node.data as { anchor?: SourceAnchor }).anchor ?? null
      if (anchor !== null) void revealAnchor(anchor)
      else if (node.type === 'entity') {
        const ent = graph.entities.get(node.id)
        if (ent !== undefined) void revealAnchor({ uri: ent.uri, span: ent.span })
      }
    },
    [graph, openBeat, revealAnchor],
  )

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
      // Beat-view body nodes: jump to source, and edit single-line items.
      const bodyAnchor = (node.data as { anchor?: SourceAnchor }).anchor ?? null
      if (bodyAnchor !== null) {
        const items = [
          { label: 'Show source', onSelect: () => void revealAnchor(bodyAnchor) },
        ]
        if (editable) {
          const text = docText(bodyAnchor.uri)
          const slice =
            text?.slice(bodyAnchor.span.start.offset, bodyAnchor.span.end.offset) ?? null
          if (slice !== null && !slice.includes('\n') && slice.trim().length > 0) {
            items.push({
              label: 'Edit text…',
              onSelect: () => void editAnchoredText(bodyAnchor, slice, say),
            })
          }
        }
        openContextMenu(items, anchor)
      }
    },
    [graph, editable, openBeat, revealBeatSource, revealAnchor, say],
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
    if (search.trim().length === 0 || positioned === null) return
    const q = search.trim().toLowerCase()
    const hit = positioned.nodes.find((n) => n.id.toLowerCase().includes(q))
    if (hit === undefined) return
    select(hit.id)
    const abs = absolutePosition(hit, positioned.nodes)
    rf.setCenter(abs.x + (hit.width ?? 100) / 2, abs.y + 40, { zoom: 1, duration: 300 })
  }, [search, positioned, rf, select])

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
            onNodeClick={onNodeClick}
            onNodeDoubleClick={onNodeDoubleClick}
            onNodeDragStop={onNodeDragStop}
            onNodeContextMenu={onNodeContextMenu}
            onPaneContextMenu={onPaneContextMenu}
            onPaneClick={() => select(null)}
            onConnect={onConnect}
            onReconnect={onReconnect}
            deleteKeyCode={null}
          >
            <Background gap={18} size={1} color="#1f2430" />
            <Controls className="!bg-zinc-900 !border-white/10" showInteractive={false} />
          </ReactFlow>
        )}
      </div>
    </div>
  )
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
  if (edge.targetRange === null) {
    say('This connection has no editable source anchor.')
    return
  }
  const target = graph.beats.get(targetId)
  if (target === undefined) {
    say('Connections land on beats.')
    return
  }
  const written = writtenTargetFor(target)
  if (written === null) {
    say('That beat is shadowed — rename one of the duplicates first.')
    return
  }
  const { uri, start, end } = edge.targetRange
  const text = docText(uri)
  if (text === null) return
  const current = text.slice(start, end)
  try {
    const edits: TextEdit[] = retargetDivert(text, start, end, current, written)
    if (edits.length === 0) return
    await applyEditsToUri(uri, edits)
    say(`Rewired → ${written}`)
  } catch (e) {
    say(e instanceof EditError ? e.message : 'Could not rewire — source changed underneath.')
  }
}

/** Create a new beat in the active (or first) `.loom` file. */
async function createBeat(graph: StoryGraph, say: (m: string) => void): Promise<void> {
  const name = window.prompt('New beat name:')
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
  useGraph.getState().select(clean)
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

/** Inline-edit a single-line body item (choice text, prose, directive). */
async function editAnchoredText(
  anchor: NonNullable<SourceAnchor>,
  current: string,
  say: (m: string) => void,
): Promise<void> {
  const next = window.prompt('Edit line:', current)
  if (next === null || next === current) return
  const text = docText(anchor.uri)
  if (text === null) return
  try {
    const edits = replaceExact(text, anchor.span.start.offset, anchor.span.end.offset, current, next)
    await applyEditsToUri(anchor.uri, edits)
  } catch (e) {
    say(e instanceof EditError ? e.message : 'Edit failed — source changed underneath.')
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
