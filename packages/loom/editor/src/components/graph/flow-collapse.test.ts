// collapseFlowEdges — the pure re-routing behind collapsed file
// containers: endpoints hidden inside a collapsed file re-route onto
// the file node, fully-internal edges drop, and re-routed parallels
// between the same pair merge into one aggregate edge with a count
// label. No React Flow, no corpus — synthetic fixtures only (the
// corpus-driven collapse pass lives in graph-pipeline.test.ts).

import { describe, expect, it } from 'vitest'
import type { Edge } from '@xyflow/react'
import { collapseFlowEdges } from './flow'

const edge = (id: string, source: string, target: string): Edge => ({ id, source, target })

// Two collapsed files: A hides a1/a2, B hides b1.
const ALIAS = new Map([
  ['a1', 'file:A'],
  ['a2', 'file:A'],
  ['b1', 'file:B'],
])

describe('collapseFlowEdges', () => {
  it('is the identity when nothing is collapsed', () => {
    const edges = [edge('e1', 'x', 'y')]
    expect(collapseFlowEdges(edges, new Map())).toBe(edges)
  })

  it('re-routes hidden endpoints onto the file node, keeping identity', () => {
    const out = collapseFlowEdges([edge('e1', 'x', 'a1'), edge('e2', 'a2', 'y')], ALIAS)
    expect(out.map((e) => [e.source, e.target])).toEqual([
      ['x', 'file:A'],
      ['file:A', 'y'],
    ])
    // A lone survivor keeps its id (and with it label/styling/payload).
    expect(out.map((e) => e.id)).toEqual(['e1', 'e2'])
  })

  it('drops edges fully internal to one collapsed file', () => {
    const out = collapseFlowEdges([edge('e1', 'a1', 'a2'), edge('e2', 'x', 'y')], ALIAS)
    expect(out.map((e) => e.id)).toEqual(['e2'])
  })

  it('keeps edges between two different collapsed files', () => {
    const out = collapseFlowEdges([edge('e1', 'a1', 'b1')], ALIAS)
    expect(out.map((e) => [e.source, e.target])).toEqual([['file:A', 'file:B']])
  })

  it('merges re-routed parallels into one counted aggregate', () => {
    const out = collapseFlowEdges(
      [edge('e1', 'x', 'a1'), edge('e2', 'x', 'a2'), edge('e3', 'x', 'a1'), edge('e4', 'x', 'y')],
      ALIAS,
    )
    expect(out).toHaveLength(2)
    const agg = out.find((e) => e.id.startsWith('collapsed:'))
    expect(agg).toBeDefined()
    expect(agg!.source).toBe('x')
    expect(agg!.target).toBe('file:A')
    expect(agg!.label).toBe('3 links')
    // No graphEdge payload — the connection inspector / reconnect skip it.
    expect(agg!.data).toBeUndefined()
    expect(out.find((e) => e.id === 'e4')?.target).toBe('y')
  })

  it('distinguishes direction when merging', () => {
    const out = collapseFlowEdges([edge('e1', 'x', 'a1'), edge('e2', 'a2', 'x')], ALIAS)
    expect(out.map((e) => [e.source, e.target])).toEqual([
      ['x', 'file:A'],
      ['file:A', 'x'],
    ])
  })

  it('leaves parallels untouched by the collapse alone', () => {
    const out = collapseFlowEdges(
      [edge('e1', 'x', 'y'), edge('e2', 'x', 'y'), edge('e3', 'x', 'a1')],
      ALIAS,
    )
    expect(out.map((e) => e.id)).toEqual(['e1', 'e2', 'e3'])
  })
})
