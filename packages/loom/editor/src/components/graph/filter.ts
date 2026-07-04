// Live canvas filter — the toolbar input dims every project-view node
// whose identity (beat key / owner, entity name, file path, ghost
// target) misses the query. Pure matching over the node payloads the
// flow builder already carries, so the canvas applies it in the
// decoration pass with no rebuild and no ELK re-run.

import type { Node } from '@xyflow/react'
import type { GraphBeat, GraphEntity } from '@loom/core/lsp'
import { pathForUri } from '@/lib/lsp-client'

/** Opacity applied to filtered-out nodes (edges go dimmer still). */
export const FILTER_DIM_OPACITY = 0.15
export const FILTER_DIM_EDGE_OPACITY = 0.08

/** Case-insensitive substring match of one node against `q` (lowercased). */
export function nodeMatchesFilter(node: Node, q: string): boolean {
  const d = node.data as Record<string, unknown>
  switch (node.type) {
    case 'beat': {
      const beat = d.beat as GraphBeat
      if (beat.key.toLowerCase().includes(q)) return true
      if (beat.owner !== null && beat.owner.toLowerCase().includes(q)) return true
      return beat.uri !== null && pathForUri(beat.uri).toLowerCase().includes(q)
    }
    case 'entity': {
      const entity = d.entity as GraphEntity
      if (entity.name.toLowerCase().includes(q)) return true
      return pathForUri(entity.uri).toLowerCase().includes(q)
    }
    case 'fileGroup':
      return typeof d.path === 'string' && d.path.toLowerCase().includes(q)
    case 'ghost':
      return typeof d.target === 'string' && d.target.toLowerCase().includes(q)
    default:
      // END and any future terminals carry no name to match.
      return false
  }
}

/**
 * The node ids to dim for `query` — `null` when the filter is inactive
 * (empty / whitespace query), so callers can skip the pass entirely.
 */
export function dimmedNodeIds(nodes: Node[], query: string): Set<string> | null {
  const q = query.trim().toLowerCase()
  if (q.length === 0) return null
  const dimmed = new Set<string>()
  for (const n of nodes) {
    if (!nodeMatchesFilter(n, q)) dimmed.add(n.id)
  }
  return dimmed
}
