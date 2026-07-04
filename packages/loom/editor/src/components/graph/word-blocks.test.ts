// The word-block projection behind the expanded beat cards: a lowered
// body flattens into typed blocks (enum kinds), nesting carried as
// depth, the size estimate is positive so ELK gets a real box, and
// every block anchors back into source (topIndex + exact edit spans —
// the 1:1 authoring contract with the Writing screenplay format).

import { describe, expect, it } from 'vitest'
import { parse } from '@loom/core/parser'
import { WordBlockKind, blockEditRange, toWordBlocks, wordBlocksHeight } from './word-blocks'

const SRC = `== opening
  The lights dim.
  GREETER
    Welcome, traveler.
  <set: guest.score += 5>
  <if: guest.score > 3>
    You feel seen.
  * Take the stairs
    -> stairs
  * Take the lift
    -> lift
`

function bodyOf(src: string, name: string) {
  const [file] = parse(src)
  for (const item of file.items) {
    if (item.kind === 'beat' && item.value.name === name) return item.value.body
  }
  throw new Error('beat not found')
}

function beatBody() {
  return bodyOf(SRC, 'opening')
}

describe('toWordBlocks', () => {
  it('flattens every body item into a typed block', () => {
    const blocks = toWordBlocks(beatBody())
    expect(blocks.map((b) => b.kind)).toEqual([
      WordBlockKind.Prose,
      WordBlockKind.Dialogue,
      WordBlockKind.Directive,
      WordBlockKind.Branch,
      WordBlockKind.Prose,
      WordBlockKind.Choice,
      WordBlockKind.Divert,
      WordBlockKind.Choice,
      WordBlockKind.Divert,
    ])
  })

  it('carries nesting as depth and labels speakers / markers / arms', () => {
    const blocks = toWordBlocks(beatBody())
    const dialogue = blocks.find((b) => b.kind === WordBlockKind.Dialogue)!
    expect(dialogue.label).toBe('GREETER')
    expect(dialogue.text).toBe('Welcome, traveler.')
    const branch = blocks.find((b) => b.kind === WordBlockKind.Branch)!
    expect(branch.label).toBe('<if: guest.score > 3>')
    const armProse = blocks[blocks.indexOf(branch) + 1]!
    expect(armProse.depth).toBe(1)
    const choice = blocks.find((b) => b.kind === WordBlockKind.Choice)!
    expect(choice.label).toBe('*')
    const divert = blocks.find((b) => b.kind === WordBlockKind.Divert)!
    expect(divert.depth).toBe(1)
    expect(divert.text).toBe('-> stairs')
  })

  it('estimates a positive pixel height for the expanded card', () => {
    const blocks = toWordBlocks(beatBody())
    expect(wordBlocksHeight(blocks)).toBeGreaterThan(blocks.length * 8)
  })
})

describe('word-block source anchors', () => {
  const blocks = toWordBlocks(beatBody())

  it('threads topIndex through nesting (children carry the ancestor index)', () => {
    // Body items: action(0) dialogue(1) directive(2) conditional(3)
    // choice(4) choice(5) — arm/choice children keep the parent index.
    expect(blocks.map((b) => b.topIndex)).toEqual([0, 1, 2, 3, 3, 4, 4, 5, 5])
  })

  it('anchors prose / directive / divert blocks to their exact source lines', () => {
    const prose = blocks[0]!
    expect(SRC.slice(prose.spanStart!, prose.spanEnd!)).toBe('The lights dim.')
    const directive = blocks.find((b) => b.kind === WordBlockKind.Directive)!
    expect(SRC.slice(directive.spanStart!, directive.spanEnd!)).toBe('<set: guest.score += 5>')
    const divert = blocks.find((b) => b.kind === WordBlockKind.Divert)!
    expect(SRC.slice(divert.spanStart!, divert.spanEnd!)).toBe('-> stairs')
  })

  it('a dialogue block edits the cue line + its merged prose, nothing more', () => {
    const dialogue = blocks.find((b) => b.kind === WordBlockKind.Dialogue)!
    const range = blockEditRange(SRC, dialogue)!
    expect(SRC.slice(range[0], range[1])).toBe('GREETER\n    Welcome, traveler.')
  })

  it('a choice block edits only its `* text` line', () => {
    const choice = blocks.find((b) => b.kind === WordBlockKind.Choice)!
    expect(choice.firstLineOnly).toBe(true)
    const range = blockEditRange(SRC, choice)!
    expect(SRC.slice(range[0], range[1])).toBe('* Take the stairs')
  })

  it('branch heads and slot holes are display-only', () => {
    const branch = blocks.find((b) => b.kind === WordBlockKind.Branch)!
    expect(branch.spanStart).toBeNull()
    expect(blockEditRange(SRC, branch)).toBeNull()
  })

  it('a bare dialogue cue (no merged prose) edits only its speaker line', () => {
    const src = `== b
  NPC
    <if: x>
      Hey.
`
    const [cue] = toWordBlocks(bodyOf(src, 'b'))
    expect(cue!.kind).toBe(WordBlockKind.Dialogue)
    expect(cue!.firstLineOnly).toBe(true)
    const range = blockEditRange(src, cue!)!
    expect(src.slice(range[0], range[1])).toBe('NPC')
  })
})
