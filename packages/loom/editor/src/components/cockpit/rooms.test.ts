// The perspective-aware room model: which rooms each lens lists, how DM
// threads fold, and which messages count toward a lens's feed.

import { describe, expect, it } from 'vitest'
import { buildRooms, lensKindOf, LensKind, messageInLens, type RoomsInput } from './rooms'
import type { CockpitMessage, RosterRow } from '@/store/cockpit'

const g1: RosterRow = { id: 'g1', name: 'Ada', role: 'Guest', faction: 'Mods', trueFaction: 'Mods', location: 'Party', captured: false, score: 0 }
const g2: RosterRow = { id: 'g2', name: 'Bo', role: 'Guest', faction: null, trueFaction: null, location: null, captured: false, score: 0 }

function msg(partial: Partial<CockpitMessage> & Pick<CockpitMessage, 'seq' | 'channel' | 'audience'>): CockpitMessage {
  return { from: '', text: 'x', kind: 'line', ts: 0, ...partial }
}

const INPUT: RoomsInput = {
  channels: [
    { id: 'lobby', kind: 'lobby', title: 'The Internet', spaceId: 'internet' },
    { id: 'faction:Mods', kind: 'faction', title: '#mods', spaceId: 'internet' },
    { id: 'loc:Party', kind: 'location', title: 'The Party', spaceId: 'internet' },
    { id: 'room:backroom', kind: 'private', title: '# the-backroom', spaceId: 'internet' },
  ],
  messages: [
    msg({ seq: 0, channel: 'lobby', audience: 'all', kind: 'narration', from: 'Narrator' }),
    msg({ seq: 1, channel: 'dm:GREETER', channelKind: 'dm', audience: ['g1'], from: 'GREETER' }),
    msg({ seq: 2, channel: 'dm:GREETER', channelKind: 'dm', audience: ['g2'], from: 'GREETER' }),
    msg({ seq: 3, channel: 'room:backroom', channelKind: 'private', audience: ['g1'] }),
  ],
  roster: [g1, g2],
  factions: [{ id: 'Mods', hidden: false, revealed: false, members: ['g1'] }],
  cast: [{ id: 'Greeter', faction: 'Mods' }],
  perspective: 'operator',
}

describe('lens resolution', () => {
  it('maps ids to operator / guest / performer lenses', () => {
    expect(lensKindOf('operator', INPUT.roster, INPUT.cast)).toBe(LensKind.Operator)
    expect(lensKindOf('g1', INPUT.roster, INPUT.cast)).toBe(LensKind.Guest)
    expect(lensKindOf('Greeter', INPUT.roster, INPUT.cast)).toBe(LensKind.Performer)
    expect(lensKindOf('nobody', INPUT.roster, INPUT.cast)).toBe(LensKind.Operator)
  })

  it('applies the audience rule only to guest lenses', () => {
    const m = msg({ seq: 9, channel: 'lobby', audience: ['g2'] })
    expect(messageInLens(m, 'operator', LensKind.Operator)).toBe(true)
    expect(messageInLens(m, 'Greeter', LensKind.Performer)).toBe(true)
    expect(messageInLens(m, 'g1', LensKind.Guest)).toBe(false)
    expect(messageInLens(m, 'g2', LensKind.Guest)).toBe(true)
  })
})

describe('buildRooms', () => {
  it('operator: every channel plus one room per (character × guest) DM thread', () => {
    const rooms = buildRooms(INPUT)
    const keys = rooms.map((r) => r.key)
    expect(keys).toContain('lobby')
    expect(keys).toContain('faction:Mods')
    expect(keys).toContain('loc:Party')
    expect(keys).toContain('room:backroom')
    expect(keys).toContain('dm:GREETER#g1')
    expect(keys).toContain('dm:GREETER#g2')
    // Per-guest DM titles carry both ends for the operator.
    expect(rooms.find((r) => r.key === 'dm:GREETER#g1')?.title).toBe('GREETER · Ada')
    // Sidebar order: lobby → faction → location → the rest.
    expect(keys.indexOf('lobby')).toBeLessThan(keys.indexOf('faction:Mods'))
    expect(keys.indexOf('faction:Mods')).toBeLessThan(keys.indexOf('loc:Party'))
    expect(keys.indexOf('loc:Party')).toBeLessThan(keys.indexOf('room:backroom'))
  })

  it("guest lens: their faction, every location, their DMs (folded), traffic-gated private rooms", () => {
    const rooms = buildRooms({ ...INPUT, perspective: 'g1' })
    const keys = rooms.map((r) => r.key)
    expect(keys).toContain('faction:Mods') // member
    expect(keys).toContain('loc:Party') // locations are open
    expect(keys).toContain('room:backroom') // has a message addressed to g1
    expect(keys).toContain('dm:GREETER#g1')
    expect(keys).not.toContain('dm:GREETER#g2') // another guest's thread
    // Their own DM folds to just the character's name.
    expect(rooms.find((r) => r.key === 'dm:GREETER#g1')?.title).toBe('GREETER')
  })

  it('guest lens: a non-member sees no faction channel and no gated room', () => {
    const rooms = buildRooms({ ...INPUT, perspective: 'g2' })
    const keys = rooms.map((r) => r.key)
    expect(keys).not.toContain('faction:Mods')
    expect(keys).not.toContain('room:backroom')
    expect(keys).toContain('dm:GREETER#g2')
    expect(keys).toContain('lobby')
  })

  it('performer lens: the full room set (they run every room)', () => {
    const rooms = buildRooms({ ...INPUT, perspective: 'Greeter' })
    const keys = rooms.map((r) => r.key)
    expect(keys).toContain('faction:Mods')
    expect(keys).toContain('room:backroom')
    expect(keys).toContain('dm:GREETER#g1')
    expect(keys).toContain('dm:GREETER#g2')
  })
})
