// nearestInDirection — the pure picker behind arrow-key selection
// walking: candidates strictly in the pressed direction, off-axis
// drift penalized double, so a straight-ahead neighbour beats a
// closer diagonal one.

import { describe, expect, it } from 'vitest'
import { arrowDirection, nearestInDirection, nearestToPoint, type NavNode } from './navigation'

const n = (id: string, cx: number, cy: number): NavNode => ({ id, cx, cy })

describe('arrowDirection', () => {
  it('maps arrow keys and nothing else', () => {
    expect(arrowDirection('ArrowUp')).toBe('up')
    expect(arrowDirection('ArrowDown')).toBe('down')
    expect(arrowDirection('ArrowLeft')).toBe('left')
    expect(arrowDirection('ArrowRight')).toBe('right')
    expect(arrowDirection('Enter')).toBeNull()
    expect(arrowDirection('a')).toBeNull()
  })
})

describe('nearestInDirection', () => {
  const from = n('from', 100, 100)

  it('only considers nodes strictly in the direction', () => {
    const candidates = [n('behind', 0, 100), n('level', 100, 100), from]
    expect(nearestInDirection(from, candidates, 'right')).toBeNull()
  })

  it('picks the nearest node in the half-plane', () => {
    const candidates = [n('near', 200, 100), n('far', 400, 100)]
    expect(nearestInDirection(from, candidates, 'right')?.id).toBe('near')
  })

  it('prefers straight-ahead over a closer diagonal', () => {
    // `diag` is nearer in raw distance but drifts 90px off-axis —
    // double-weighted, it loses to the aligned node.
    const candidates = [n('diag', 150, 190), n('ahead', 260, 100)]
    expect(nearestInDirection(from, candidates, 'right')?.id).toBe('ahead')
  })

  it('works on the vertical axis', () => {
    const candidates = [n('above', 100, 20), n('below', 100, 300)]
    expect(nearestInDirection(from, candidates, 'up')?.id).toBe('above')
    expect(nearestInDirection(from, candidates, 'down')?.id).toBe('below')
    expect(nearestInDirection(from, [n('above', 100, 20)], 'down')).toBeNull()
  })

  it('never returns the origin node itself', () => {
    expect(nearestInDirection(from, [from, n('r', 300, 100)], 'right')?.id).toBe('r')
  })
})

describe('nearestToPoint', () => {
  it('seeds from the closest candidate', () => {
    const candidates = [n('a', 0, 0), n('b', 50, 50), n('c', 500, 500)]
    expect(nearestToPoint({ x: 60, y: 40 }, candidates)?.id).toBe('b')
  })

  it('handles an empty walk', () => {
    expect(nearestToPoint({ x: 0, y: 0 }, [])).toBeNull()
  })
})
