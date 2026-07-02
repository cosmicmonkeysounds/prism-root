// ELK layered layout for the story-graph canvas.
//
// One async entry point: hand it flat nodes (optionally parented into
// file-group containers) + edges, get back per-node positions and the
// computed container sizes. ELK's `INCLUDE_CHILDREN` hierarchy handling
// routes cross-container edges (a divert from `arrival.loom` into
// `algorithm.loom`) through the compound layout instead of ignoring
// them — the property that makes the global view legible.
//
// Positions come back RELATIVE to the parent container, which is
// exactly what React Flow subflows (`parentId` + child coordinates)
// expect — no re-basing needed.

import ELK from 'elkjs/lib/elk.bundled.js'
import type { ElkExtendedEdge, ElkNode } from 'elkjs/lib/elk.bundled.js'

export interface LayoutNodeIn {
  id: string
  width: number
  height: number
  /** File-group container id, when nested. */
  parentId?: string
  /** True for the container nodes themselves. */
  isGroup?: boolean
}

export interface LayoutEdgeIn {
  id: string
  source: string
  target: string
}

export interface LayoutOut {
  /** Node id → position (children relative to their container). */
  positions: Map<string, { x: number; y: number }>
  /** Container id → computed size. */
  groupSizes: Map<string, { width: number; height: number }>
}

const elk = new ELK()

const GROUP_PADDING = { top: 44, left: 16, right: 16, bottom: 16 }

/** Layered layout, `RIGHT` for the project flow, `DOWN` for beat interiors. */
export async function layeredLayout(
  nodes: LayoutNodeIn[],
  edges: LayoutEdgeIn[],
  direction: 'RIGHT' | 'DOWN' = 'RIGHT',
): Promise<LayoutOut> {
  const byParent = new Map<string | undefined, LayoutNodeIn[]>()
  for (const n of nodes) {
    if (n.isGroup) continue
    const list = byParent.get(n.parentId)
    if (list) list.push(n)
    else byParent.set(n.parentId, [n])
  }

  const groups = nodes.filter((n) => n.isGroup)
  const child = (n: LayoutNodeIn): ElkNode => ({
    id: n.id,
    width: n.width,
    height: n.height,
  })

  const root: ElkNode = {
    id: '__root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': direction,
      'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
      'elk.layered.spacing.nodeNodeBetweenLayers': '56',
      'elk.spacing.nodeNode': '28',
      'elk.spacing.componentComponent': '48',
      'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
      'elk.padding': '[top=8,left=8,bottom=8,right=8]',
    },
    children: [
      ...groups.map((g) => ({
        id: g.id,
        layoutOptions: {
          'elk.padding': `[top=${GROUP_PADDING.top},left=${GROUP_PADDING.left},bottom=${GROUP_PADDING.bottom},right=${GROUP_PADDING.right}]`,
        },
        children: (byParent.get(g.id) ?? []).map(child),
      })),
      ...(byParent.get(undefined) ?? []).map(child),
    ],
    edges: edges.map(
      (e): ElkExtendedEdge => ({ id: e.id, sources: [e.source], targets: [e.target] }),
    ),
  }

  const res = await elk.layout(root)

  const positions = new Map<string, { x: number; y: number }>()
  const groupSizes = new Map<string, { width: number; height: number }>()
  const groupIds = new Set(groups.map((g) => g.id))
  const walk = (node: ElkNode): void => {
    for (const c of node.children ?? []) {
      positions.set(c.id, { x: c.x ?? 0, y: c.y ?? 0 })
      if (groupIds.has(c.id)) {
        groupSizes.set(c.id, { width: c.width ?? 0, height: c.height ?? 0 })
      }
      walk(c)
    }
  }
  walk(res)
  return { positions, groupSizes }
}
