// Cursor-follow resolution: nodeAtLine maps a text position to the
// enclosing canvas node (beat / entity) by nearest-section-start.

import { describe, expect, it } from 'vitest'
import { Workspace } from '@loom/core/lsp'
import { nodeAtLine } from './follow'

const URI = 'inmemory:///main.loom'

// Line numbers are 0-based; keep this fixture stable — the assertions
// below index into it.
const MAIN = `entry: opening

FACTION Mods
  ethos: order

CHARACTER Greeter
  faction: Mods
  on scan guest
    <set: guest.score += 5>
  beat aside
    A quiet word.

== opening
  The lights dim.
  GREETER
    Welcome, traveler.
  -> summit

== summit
  The city glitters below.
  -> END
`

function graphFor(): ReturnType<Workspace['storyGraph']> {
  const ws = new Workspace()
  ws.updateMany([[URI, MAIN]])
  return ws.storyGraph()
}

describe('nodeAtLine', () => {
  const graph = graphFor()

  it('resolves lines inside a top-level beat to that beat', () => {
    expect(nodeAtLine(graph, URI, 13)).toBe('opening') // "The lights dim."
    expect(nodeAtLine(graph, URI, 16)).toBe('opening') // "-> summit"
    expect(nodeAtLine(graph, URI, 19)).toBe('summit')
  })

  it('resolves the declaration line itself', () => {
    expect(nodeAtLine(graph, URI, 12)).toBe('opening') // "== opening"
  })

  it('resolves character-body lines to the entity, not a beat', () => {
    expect(nodeAtLine(graph, URI, 6)).toBe('character:Greeter') // "faction: Mods"
    expect(nodeAtLine(graph, URI, 8)).toBe('character:Greeter') // hook body
  })

  it('resolves an owned beat block to the owned beat', () => {
    // "beat aside" starts inside CHARACTER Greeter.
    expect(nodeAtLine(graph, URI, 10)).toBe('Greeter.aside') // "A quiet word."
  })

  it('returns null above every declaration and for unknown files', () => {
    expect(nodeAtLine(graph, URI, 0)).toBeNull() // "entry:" header
    expect(nodeAtLine(graph, 'inmemory:///other.loom', 5)).toBeNull()
  })

  it('resolves the FACTION block to its entity', () => {
    expect(nodeAtLine(graph, URI, 3)).toBe('faction:Mods')
  })
})
