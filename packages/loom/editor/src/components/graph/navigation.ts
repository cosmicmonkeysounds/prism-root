// Keyboard spatial navigation over the canvas — pure geometry so the
// picker is unit-testable without React Flow. The canvas feeds it
// absolute node midpoints (file containers only when collapsed,
// filter-dimmed nodes excluded) and moves selection to the winner.

export type NavDirection = 'up' | 'down' | 'left' | 'right'

/** Arrow-key → direction, `null` for any other key. */
export function arrowDirection(key: string): NavDirection | null {
  switch (key) {
    case 'ArrowUp':
      return 'up'
    case 'ArrowDown':
      return 'down'
    case 'ArrowLeft':
      return 'left'
    case 'ArrowRight':
      return 'right'
    default:
      return null
  }
}

export interface NavNode {
  id: string
  /** Absolute canvas midpoint. */
  cx: number
  cy: number
}

/**
 * The nearest candidate strictly in `dir` from `from` — weighted
 * Manhattan distance with off-axis drift counted double, so a
 * straight-ahead node beats a closer diagonal one. `null` when nothing
 * lies in that direction.
 */
export function nearestInDirection(
  from: NavNode,
  candidates: NavNode[],
  dir: NavDirection,
): NavNode | null {
  let best: NavNode | null = null
  let bestScore = Infinity
  for (const c of candidates) {
    if (c.id === from.id) continue
    const dx = c.cx - from.cx
    const dy = c.cy - from.cy
    const ahead = dir === 'right' ? dx : dir === 'left' ? -dx : dir === 'down' ? dy : -dy
    if (ahead <= 0) continue
    const cross = dir === 'left' || dir === 'right' ? Math.abs(dy) : Math.abs(dx)
    const score = ahead + cross * 2
    if (score < bestScore) {
      bestScore = score
      best = c
    }
  }
  return best
}

/** The candidate closest to `point` (seed when nothing is selected). */
export function nearestToPoint(
  point: { x: number; y: number },
  candidates: NavNode[],
): NavNode | null {
  let best: NavNode | null = null
  let bestDist = Infinity
  for (const c of candidates) {
    const d = (c.cx - point.x) ** 2 + (c.cy - point.y) ** 2
    if (d < bestDist) {
      bestDist = d
      best = c
    }
  }
  return best
}
