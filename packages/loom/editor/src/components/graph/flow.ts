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
  GHOST_H,
  GHOST_W,
  beatNodeSize,
} from './metrics'
import type { LayoutEdgeIn, LayoutNodeIn } from './layout'

export const END_ID = '__end'
export const ghostId = (target: string): string => `ghost:${target}`
export const groupId = (uri: string): string => `file:${uri}`

export interface ProjectFlow {
  nodes: Node[]
  layoutNodes: LayoutNodeIn[]
  layoutEdges: LayoutEdgeIn[]
  edges: Edge[]
}

/** Build the project-level flow (positions come from ELK afterwards). */
export function buildProjectFlow(graph: StoryGraph, overlays: GraphOverlays): ProjectFlow {
  const nodes: Node[] = []
  const layoutNodes: LayoutNodeIn[] = []
  const layoutEdges: LayoutEdgeIn[] = []
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
  for (const f of graph.files) {
    if (!usedFiles.has(f.uri)) continue
    const id = groupId(f.uri)
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
    const size = beatNodeSize(beat)
    const parentUri = fileOf.get(key)
    const parent = parentUri !== undefined && usedFiles.has(parentUri) ? groupId(parentUri) : undefined
    nodes.push({
      id: key,
      type: 'beat',
      position: { x: 0, y: 0 },
      data: { beat },
      ...(parent !== undefined ? { parentId: parent } : {}),
      ...(parent !== undefined ? { extent: 'parent' as const } : {}),
    })
    layoutNodes.push({ id: key, width: size.width, height: size.height, parentId: parent })
  }

  // Entities.
  for (const [id, entity] of graph.entities) {
    if (!visibleEntity(id)) continue
    const parentUri = fileOf.get(id)
    const parent = parentUri !== undefined && usedFiles.has(parentUri) ? groupId(parentUri) : undefined
    nodes.push({
      id,
      type: 'entity',
      position: { x: 0, y: 0 },
      data: { entity },
      ...(parent !== undefined ? { parentId: parent } : {}),
      ...(parent !== undefined ? { extent: 'parent' as const } : {}),
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
    layoutEdges.push({ id: e.id, source: e.from, target })
  }

  for (const t of ghosts) {
    nodes.push({
      id: ghostId(t),
      type: 'ghost',
      position: { x: 0, y: 0 },
      data: { target: t, from: '' },
    })
    layoutNodes.push({ id: ghostId(t), width: GHOST_W, height: GHOST_H })
  }
  if (needEnd) {
    nodes.push({ id: END_ID, type: 'end', position: { x: 0, y: 0 }, data: {} })
    layoutNodes.push({ id: END_ID, width: END_W, height: END_H })
  }

  return { nodes, layoutNodes, layoutEdges, edges }
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
