// Floating edge for the PROJECT view — anchors each end to the point
// where the straight line between the two node centers crosses the node
// border, instead of fixed left/right handles. With nodes freely
// draggable this is what keeps connectors sensible: drag a beat to the
// left of its source and the edge leaves from the source's left side
// rather than looping around from its right handle.
//
// Adapted from the xyflow "floating edges" example, trimmed to our
// needs (bezier path + optional label via EdgeLabelRenderer).

import { memo } from 'react'
import {
  BaseEdge,
  EdgeLabelRenderer,
  getBezierPath,
  Position,
  useInternalNode,
  type EdgeProps,
  type InternalNode,
  type Node,
} from '@xyflow/react'

/** Center of a laid-out node in absolute canvas coordinates. */
function centerOf(node: InternalNode<Node>): { x: number; y: number } {
  const { x, y } = node.internals.positionAbsolute
  return {
    x: x + (node.measured.width ?? 0) / 2,
    y: y + (node.measured.height ?? 0) / 2,
  }
}

/** Where the line toward `other` crosses `node`'s border rectangle. */
function borderIntersection(
  node: InternalNode<Node>,
  other: InternalNode<Node>,
): { x: number; y: number } {
  const w = (node.measured.width ?? 0) / 2
  const h = (node.measured.height ?? 0) / 2
  const c = centerOf(node)
  const o = centerOf(other)

  const xx1 = (o.x - c.x) / (2 * w) - (o.y - c.y) / (2 * h)
  const yy1 = (o.x - c.x) / (2 * w) + (o.y - c.y) / (2 * h)
  const a = 1 / (Math.abs(xx1) + Math.abs(yy1) || 1)
  const xx3 = a * xx1
  const yy3 = a * yy1
  return { x: w * (xx3 + yy3) + c.x, y: h * (-xx3 + yy3) + c.y }
}

/** Which side of the node a border point sits on (for bezier control). */
function sideOf(node: InternalNode<Node>, point: { x: number; y: number }): Position {
  const { x: nx, y: ny } = node.internals.positionAbsolute
  const w = node.measured.width ?? 0
  const h = node.measured.height ?? 0
  const px = Math.round(point.x)
  const py = Math.round(point.y)
  if (px <= Math.round(nx) + 1) return Position.Left
  if (px >= Math.round(nx + w) - 1) return Position.Right
  if (py <= Math.round(ny) + 1) return Position.Top
  if (py >= Math.round(ny + h) - 1) return Position.Bottom
  return Position.Right
}

export const FloatingEdge = memo(function FloatingEdge({
  id,
  source,
  target,
  style,
  markerEnd,
  label,
  labelStyle,
  interactionWidth,
}: EdgeProps) {
  const sourceNode = useInternalNode(source)
  const targetNode = useInternalNode(target)
  if (sourceNode === undefined || targetNode === undefined) return null

  const sp = borderIntersection(sourceNode, targetNode)
  const tp = borderIntersection(targetNode, sourceNode)
  const [path, labelX, labelY] = getBezierPath({
    sourceX: sp.x,
    sourceY: sp.y,
    sourcePosition: sideOf(sourceNode, sp),
    targetX: tp.x,
    targetY: tp.y,
    targetPosition: sideOf(targetNode, tp),
  })

  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        style={style}
        markerEnd={markerEnd}
        interactionWidth={interactionWidth}
      />
      {label != null && label !== '' && (
        <EdgeLabelRenderer>
          <div
            className="pointer-events-none absolute rounded bg-[#101014]/80 px-1 text-[9px]"
            style={{
              transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
              color: (labelStyle?.fill as string | undefined) ?? '#a1a1aa',
            }}
          >
            {label}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  )
})
