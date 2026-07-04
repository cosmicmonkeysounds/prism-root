// dimmedNodeIds — the pure matcher behind the live canvas filter:
// case-insensitive substring over beat key / owner / file path, entity
// name / file path, container path, ghost target; a blank query
// deactivates the pass entirely.

import { describe, expect, it } from 'vitest'
import type { Node } from '@xyflow/react'
import type { GraphBeat, GraphEntity } from '@loom/core/lsp'
import { dimmedNodeIds } from './filter'

function beatNode(key: string, owner: string | null, uri: string | null): Node {
  const beat: GraphBeat = {
    key,
    name: owner === null ? key : (key.split('.')[1] ?? key),
    owner,
    params: [],
    cast: [],
    setting: null,
    uri,
    span: null,
    structural: 'file',
    entry: false,
    shadowed: false,
    counts: { dialogues: 0, choices: 0, diverts: 0 },
    preview: [],
    tunnelReturn: false,
  }
  return { id: key, type: 'beat', position: { x: 0, y: 0 }, data: { beat } }
}

function entityNode(id: string, name: string, uri: string): Node {
  const zero = { line: 0, column: 0, offset: 0 }
  const entity: GraphEntity = {
    id,
    kind: 'character',
    name,
    uri,
    span: { start: zero, end: zero },
    mixins: [],
    faction: null,
    ownedBeats: [],
    hookCount: 0,
  }
  return { id, type: 'entity', position: { x: 0, y: 0 }, data: { entity } }
}

const NODES: Node[] = [
  beatNode('opening', null, 'inmemory://main.loom'),
  beatNode('TheAdmin.tally', 'TheAdmin', 'inmemory://cast/admin.loom'),
  entityNode('character:Greeter', 'Greeter', 'inmemory://main.loom'),
  {
    id: 'file:inmemory://main.loom',
    type: 'fileGroup',
    position: { x: 0, y: 0 },
    data: { path: 'main.loom', beatCount: 1 },
  },
  { id: 'ghost:missing_beat', type: 'ghost', position: { x: 0, y: 0 }, data: { target: 'missing_beat' } },
  { id: '__end', type: 'end', position: { x: 0, y: 0 }, data: {} },
]

describe('dimmedNodeIds', () => {
  it('is inactive on a blank / whitespace query', () => {
    expect(dimmedNodeIds(NODES, '')).toBeNull()
    expect(dimmedNodeIds(NODES, '   ')).toBeNull()
  })

  it('dims everything missing a case-insensitive substring', () => {
    const dimmed = dimmedNodeIds(NODES, 'OPEN')!
    expect(dimmed.has('opening')).toBe(false)
    expect(dimmed.has('TheAdmin.tally')).toBe(true)
    expect(dimmed.has('character:Greeter')).toBe(true)
    expect(dimmed.has('file:inmemory://main.loom')).toBe(true)
  })

  it('matches beats by owner', () => {
    expect(dimmedNodeIds(NODES, 'theadmin')!.has('TheAdmin.tally')).toBe(false)
  })

  it('matches beats, entities, and containers by file path', () => {
    const dimmed = dimmedNodeIds(NODES, 'main.loom')!
    expect(dimmed.has('opening')).toBe(false)
    expect(dimmed.has('character:Greeter')).toBe(false)
    expect(dimmed.has('file:inmemory://main.loom')).toBe(false)
    expect(dimmed.has('TheAdmin.tally')).toBe(true)
  })

  it('matches ghosts by target and always dims END', () => {
    const dimmed = dimmedNodeIds(NODES, 'missing')!
    expect(dimmed.has('ghost:missing_beat')).toBe(false)
    expect(dimmed.has('__end')).toBe(true)
  })
})
