// StoryGraph → React Flow projection for the PROJECT view.
//
// Pure functions: given the core graph + overlay toggles, produce the
// node/edge arrays (file containers as subflows, beats as cards,
// entities as pills, unresolved targets as ghost nodes, one END
// terminal) and the ELK sizing inputs. The canvas component owns
// layout + interaction; this module owns shape.

import { MarkerType, type Edge, type Node } from '@xyflow/react'
import type { GraphEdge, StoryGraph } from '@loom/core/lsp'
import { pathForUri } from '@/lib/lsp-client'
import type { GraphOverlays } from '@/store/graph'
import {
  END_H,
  END_W,
  ENTITY_H,
  ENTITY_W,
  FILE_COLLAPSED_H,
  FILE_COLLAPSED_W,
  GHOST_H,
  GHOST_W,
  beatNodeSize,
} from './metrics'
import type { LayoutEdgeIn, LayoutNodeIn } from './layout'
import { wordBlocksHeight, type WordBlock } from './word-blocks'

export const END_ID = '__end'
export const ghostId = (target: string): string => `ghost:${target}`
export const groupId = (uri: string): string => `file:${uri}`

export interface ProjectFlow {
  nodes: Node[]
  layoutNodes: LayoutNodeIn[]
  layoutEdges: LayoutEdgeIn[]
  edges: Edge[]
}

/**
 * Build the project-level flow (positions come from ELK afterwards).
 * `expandedBlocks` carries the word-block body for every beat whose
 * card is expanded — it drives both the node payload and the ELK size
 * estimate, so layout always accounts for the stretched cards.
 * `collapsedFiles` (path-keyed) shrinks a file container to a compact
 * leaf node: its beats/entities are omitted and every edge touching
 * them re-routes onto the file node (parallels merged with a count).
 */
export function buildProjectFlow(
  graph: StoryGraph,
  overlays: GraphOverlays,
  expandedBlocks: Map<string, WordBlock[]> = new Map(),
  collapsedFiles: ReadonlySet<string> = new Set(),
): ProjectFlow {
  const nodes: Node[] = []
  const layoutNodes: LayoutNodeIn[] = []
  const edges: Edge[] = []

  // Which entities take part in the visible graph?
  const hookSources = new Set<string>()
  if (overlays.hooks) {
    for (const e of graph.edges) {
      if (e.kind === 'hook') hookSources.add(e.from)
    }
  }
  const visibleEntity = (id: string): boolean =>
    overlays.entities || hookSources.has(id)

  // File containers first (parents must precede children for React Flow).
  const usedFiles = new Set<string>()
  for (const f of graph.files) {
    const beatsHere = f.beats.filter((k) => graph.beats.has(k))
    const entsHere = f.entities.filter(visibleEntity)
    if (beatsHere.length + entsHere.length > 0) usedFiles.add(f.uri)
  }
  // Collapsed containers become plain leaf nodes: no children, fixed
  // compact size, and (below) every edge into their contents re-routed
  // onto the file node itself.
  const collapsedUris = new Set<string>()
  for (const f of graph.files) {
    if (usedFiles.has(f.uri) && collapsedFiles.has(pathForUri(f.uri))) collapsedUris.add(f.uri)
  }
  for (const f of graph.files) {
    if (!usedFiles.has(f.uri)) continue
    const id = groupId(f.uri)
    if (collapsedUris.has(f.uri)) {
      nodes.push({
        id,
        type: 'fileGroup',
        position: { x: 0, y: 0 },
        data: { path: pathForUri(f.uri), beatCount: f.beats.length, collapsed: true },
        selectable: true,
        draggable: true,
        style: { width: FILE_COLLAPSED_W, height: FILE_COLLAPSED_H },
      })
      layoutNodes.push({ id, width: FILE_COLLAPSED_W, height: FILE_COLLAPSED_H })
      continue
    }
    nodes.push({
      id,
      type: 'fileGroup',
      position: { x: 0, y: 0 },
      data: { path: pathForUri(f.uri), beatCount: f.beats.length },
      selectable: true,
      draggable: true,
      zIndex: -1,
    })
    layoutNodes.push({ id, width: 0, height: 0, isGroup: true })
  }

  // Beats.
  const fileOf = new Map<string, string>()
  for (const f of graph.files) {
    for (const k of f.beats) fileOf.set(k, f.uri)
    for (const k of f.entities) fileOf.set(k, f.uri)
  }
  for (const [key, beat] of graph.beats) {
    const parentUri = fileOf.get(key)
    if (parentUri !== undefined && collapsedUris.has(parentUri)) continue
    const blocks = expandedBlocks.get(key) ?? null
    const size = beatNodeSize(beat, blocks !== null ? wordBlocksHeight(blocks) : null)
    const parent = parentUri !== undefined && usedFiles.has(parentUri) ? groupId(parentUri) : undefined
    nodes.push({
      id: key,
      type: 'beat',
      position: { x: 0, y: 0 },
      data: { beat, blocks },
      ...(parent !== undefined ? { parentId: parent } : {}),
      // Children can be dragged past their file container's edge — the
      // container grows to keep holding them (vs. the old hard clamp).
      ...(parent !== undefined ? { expandParent: true as const } : {}),
    })
    layoutNodes.push({ id: key, width: size.width, height: size.height, parentId: parent })
  }

  // Entities.
  for (const [id, entity] of graph.entities) {
    if (!visibleEntity(id)) continue
    const parentUri = fileOf.get(id)
    if (parentUri !== undefined && collapsedUris.has(parentUri)) continue
    const parent = parentUri !== undefined && usedFiles.has(parentUri) ? groupId(parentUri) : undefined
    nodes.push({
      id,
      type: 'entity',
      position: { x: 0, y: 0 },
      data: { entity },
      ...(parent !== undefined ? { parentId: parent } : {}),
      ...(parent !== undefined ? { expandParent: true as const } : {}),
    })
    layoutNodes.push({ id, width: ENTITY_W, height: ENTITY_H, parentId: parent })
  }

  // Edges (+ ghost / END terminals discovered along the way).
  const ghosts = new Set<string>()
  let needEnd = false
  for (const e of graph.edges) {
    if (!edgeVisible(e, overlays)) continue
    let target: string
    if (e.kind === 'end') {
      target = END_ID
      needEnd = true
    } else if (e.to !== null) {
      target = e.to
      // Secondary edges may point at entities the overlay hides.
      if (!e.narrative && !graph.beats.has(target) && !visibleEntity(target)) continue
      if (e.narrative && e.kind === 'hook' && !visibleEntity(e.from)) continue
    } else {
      const t = e.unresolved ?? '?'
      target = ghostId(t)
      ghosts.add(t)
    }
    if (e.kind === 'hook' && !graph.beats.has(e.from) && !visibleEntity(e.from)) continue
    edges.push(flowEdge(e, target, overlays))
  }

  for (const t of ghosts) {
    nodes.push({
      id: ghostId(t),
      type: 'ghost',
      position: { x: 0, y: 0 },
      data: { target: t },
    })
    layoutNodes.push({ id: ghostId(t), width: GHOST_W, height: GHOST_H })
  }
  if (needEnd) {
    nodes.push({ id: END_ID, type: 'end', position: { x: 0, y: 0 }, data: {} })
    layoutNodes.push({ id: END_ID, width: END_W, height: END_H })
  }

  // Hidden endpoints (inside a collapsed file) re-route to the file node.
  const alias = new Map<string, string>()
  for (const f of graph.files) {
    if (!collapsedUris.has(f.uri)) continue
    for (const k of f.beats) alias.set(k, groupId(f.uri))
    for (const k of f.entities) alias.set(k, groupId(f.uri))
  }
  const routed = collapseFlowEdges(edges, alias)
  const layoutEdges: LayoutEdgeIn[] = routed.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
  }))

  return { nodes, layoutNodes, layoutEdges, edges: routed }
}

/**
 * Re-route every edge with an endpoint inside a collapsed file onto the
 * file node (`alias`: hidden node id → file node id). Edges fully
 * internal to one collapsed file drop; re-routed parallels between the
 * same pair merge into a single aggregate edge with a count label.
 * Pure — unit-tested without React Flow.
 */
export function collapseFlowEdges(edges: Edge[], alias: ReadonlyMap<string, string>): Edge[] {
  if (alias.size === 0) return edges
  const out: Edge[] = []
  const merged = new Map<string, { first: Edge; count: number; slot: number }>()
  for (const e of edges) {
    const source = alias.get(e.source) ?? e.source
    const target = alias.get(e.target) ?? e.target
    if (source === e.source && target === e.target) {
      out.push(e)
      continue
    }
    if (source === target) continue // both ends inside one collapsed file
    const key = `${source}→${target}`
    const bucket = merged.get(key)
    if (bucket === undefined) {
      // Placeholder keeps edge order; replaced once the count is known.
      merged.set(key, { first: { ...e, source, target }, count: 1, slot: out.length })
      out.push(e)
    } else {
      bucket.count += 1
    }
  }
  for (const { first, count, slot } of merged.values()) {
    out[slot] = count === 1 ? first : aggregateEdge(first, count)
  }
  return out
}

/** One edge standing in for `count` re-routed parallels. */
function aggregateEdge(first: Edge, count: number): Edge {
  return {
    id: `collapsed:${first.source}→${first.target}`,
    type: 'floating',
    source: first.source,
    target: first.target,
    label: `${count} links`,
    labelStyle: { fill: '#a1a1aa', fontSize: 9 },
    labelBgStyle: { fill: '#101014', fillOpacity: 0.8 },
    style: { stroke: '#94a3b8', strokeWidth: 1.8, opacity: 0.9 },
    markerEnd: { type: MarkerType.ArrowClosed, color: '#94a3b8', width: 15, height: 15 },
    interactionWidth: 12,
    // No graphEdge payload — an aggregate has no single source divert,
    // so the connection inspector / reconnect handlers skip it.
  }
}

function edgeVisible(e: GraphEdge, overlays: GraphOverlays): boolean {
  if (e.narrative) {
    if (e.kind === 'hook') return overlays.hooks
    return true
  }
  return overlays.entities
}

// ---------------------------------------------------------------------------
// Edge styling
// ---------------------------------------------------------------------------

const EDGE_COLOR: Record<string, string> = {
  divert: '#94a3b8',
  choice: '#34d399',
  tunnel: '#a78bfa',
  end: '#fb7185',
  hook: '#22d3ee',
  cast: '#155e75',
  setting: '#92600a',
  member: '#9f1239',
  contains: '#92600a',
  is: '#a21caf',
  owns: '#0f766e',
}

const LABEL_MAX = 26

export function flowEdge(e: GraphEdge, target: string, overlays: GraphOverlays): Edge {
  const color = e.to === null && e.kind !== 'end' ? '#f87171' : (EDGE_COLOR[e.kind] ?? '#94a3b8')
  const dash =
    e.kind === 'tunnel' ? '6 3'
    : e.kind === 'hook' ? '3 3'
    : e.dynamic ? '1 4'
    : !e.narrative ? '2 5'
    : e.to === null && e.kind !== 'end' ? '4 3'
    : undefined

  let label: string | undefined
  if (overlays.labels) {
    const parts: string[] = []
    if (e.label !== null) {
      const txt = e.sticky === true ? `↻ ${e.label}` : e.label
      parts.push(txt.length > LABEL_MAX ? `${txt.slice(0, LABEL_MAX - 1)}…` : txt)
    }
    if (e.condition !== null) {
      const c = e.condition.length > LABEL_MAX ? `${e.condition.slice(0, LABEL_MAX - 1)}…` : e.condition
      parts.push(`⟨${c}⟩`)
    }
    if (parts.length > 0) label = parts.join(' ')
  }

  return {
    id: e.id,
    // Floating edges anchor to the closest border point of each node, so
    // connectors stay sensible however the writer rearranges the map.
    type: 'floating',
    source: e.from,
    target,
    label,
    labelStyle: { fill: '#a1a1aa', fontSize: 9 },
    labelBgStyle: { fill: '#101014', fillOpacity: 0.8 },
    style: {
      stroke: color,
      strokeWidth: e.narrative ? 1.5 : 0.9,
      strokeDasharray: dash,
      opacity: e.narrative ? 0.9 : 0.45,
    },
    markerEnd: e.narrative
      ? { type: MarkerType.ArrowClosed, color, width: 15, height: 15 }
      : undefined,
    interactionWidth: 12,
    data: { graphEdge: e },
  }
}
