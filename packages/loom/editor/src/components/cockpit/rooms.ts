//! Shared room model for the Sim/Run cockpit. The rooms rail (left)
//! and the Chat tab (center) both need the same derived list —
//! authored/derived channels (incl. `loc:` location rooms) plus one room
//! per (character × guest) DM seen in the feed — so it lives here as a
//! hook over whichever cockpit store the enclosing provider supplies.
//!
//! Everything is perspective-aware: the cockpit's `perspective` lens
//! ('operator' god view, a guest id, or a character id) filters which
//! rooms are listed and which messages count. The core rules mirror the
//! engine's (`visibleTo` audience checks + channel visibility); authored
//! membership-gated rooms fall back to traffic (a visible message) since
//! the operator snapshot doesn't carry per-guest membership.

import { useMemo } from 'react'
import {
  OPERATOR_LENS,
  useCockpit,
  type CastSummary,
  type ChannelSummary,
  type CockpitMessage,
  type FactionSummary,
  type RosterRow,
} from '@/store/cockpit'

export interface Room {
  key: string
  channel: string
  kind: string
  title: string
  /** For a per-guest DM room, the single guest on the other end. */
  dmGuest: string | null
  /** For a DM room, the character (as it appears in the channel id) whose thread this is. */
  character: string | null
}

/** What the lens id resolves to. */
export const LensKind = {
  Operator: 'operator',
  Guest: 'guest',
  Performer: 'performer',
} as const

export type LensKind = (typeof LensKind)[keyof typeof LensKind]

export function lensKindOf(perspective: string, roster: RosterRow[], cast: CastSummary[]): LensKind {
  if (perspective !== OPERATOR_LENS) {
    if (roster.some((r) => r.id === perspective)) return LensKind.Guest
    if (cast.some((c) => c.id === perspective)) return LensKind.Performer
  }
  return LensKind.Operator
}

/** Is a message inside the lens's view? Operator + performers see the full
 *  feed (they run the event); a guest lens applies the audience rule. */
export function messageInLens(m: CockpitMessage, perspective: string, lens: LensKind): boolean {
  if (lens !== LensKind.Guest) return true
  return m.audience === 'all' || (Array.isArray(m.audience) && m.audience.includes(perspective))
}

/** The single guest on the far side of a DM message, or null if it isn't a 1:1 DM. */
export function dmGuestOf(m: CockpitMessage): string | null {
  const isDm = m.channelKind === 'dm' || m.channel.startsWith('dm:')
  if (isDm && Array.isArray(m.audience) && m.audience.length === 1) return m.audience[0]!
  return null
}

/** Sidebar sort: lobby, factions, locations, announcements, then the rest. */
export function roomOrder(kind: string | undefined): number {
  switch (kind) {
    case 'lobby':
      return 0
    case 'faction':
      return 1
    case 'location':
      return 2
    case 'announcement':
      return 3
    default:
      return 4
  }
}

/** The room key a message belongs to (per-guest for DMs, else the channel id). */
export function roomKeyOf(m: CockpitMessage): string {
  const gid = dmGuestOf(m)
  return gid ? `${m.channel}#${gid}` : m.channel
}

export interface RoomsInput {
  channels: ChannelSummary[]
  messages: CockpitMessage[]
  roster: RosterRow[]
  factions: FactionSummary[]
  cast: CastSummary[]
  perspective: string
}

/** Can the lens open a listed channel? Operator/performer: everything.
 *  A guest: the lobby, locations + open rooms, their faction's channel, and
 *  any gated room where some message is addressed to them (traffic proxy for
 *  membership, which the mod snapshot doesn't carry). */
function channelInLens(c: ChannelSummary, input: RoomsInput, lens: LensKind): boolean {
  if (lens !== LensKind.Guest) return true
  switch (c.kind) {
    case 'lobby':
    case 'location':
    case 'open':
    case 'announcement':
      return true
    case 'faction': {
      // Derived `faction:<F>` rooms carry the faction in the id; authored
      // faction lounges fall through to the traffic check below.
      const derived = c.id.startsWith('faction:') ? c.id.slice('faction:'.length) : null
      if (derived !== null) {
        return input.factions.some((f) => f.id === derived && f.members.includes(input.perspective))
      }
      return hasVisibleTraffic(c.id, input)
    }
    default:
      return hasVisibleTraffic(c.id, input)
  }
}

function hasVisibleTraffic(channel: string, input: RoomsInput): boolean {
  return input.messages.some(
    (m) => m.channel === channel && messageInLens(m, input.perspective, LensKind.Guest),
  )
}

/** The derived rooms list, sorted for the sidebar. Pure — unit-testable. */
export function buildRooms(input: RoomsInput): Room[] {
  const { channels, messages, roster, cast, perspective } = input
  const lens = lensKindOf(perspective, roster, cast)
  const nameOf = (gid: string) => roster.find((r) => r.id === gid)?.name ?? gid
  const map = new Map<string, Room>()
  for (const c of channels) {
    if (!channelInLens(c, input, lens)) continue
    map.set(c.id, { key: c.id, channel: c.id, kind: c.kind, title: c.title, dmGuest: null, character: null })
  }
  for (const m of messages) {
    if (!messageInLens(m, perspective, lens)) continue
    const gid = dmGuestOf(m)
    const character = m.channel.startsWith('dm:') ? m.channel.slice(3) : (m.title ?? m.channel)
    if (gid) {
      // A guest lens folds their own DM threads to just the character name.
      const key = `${m.channel}#${gid}`
      if (!map.has(key)) {
        const title = lens === LensKind.Guest && gid === perspective ? character : `${character} · ${nameOf(gid)}`
        map.set(key, { key, channel: m.channel, kind: 'dm', title, dmGuest: gid, character })
      }
    } else if (!map.has(m.channel)) {
      const kind = m.channelKind ?? 'dm'
      map.set(m.channel, {
        key: m.channel,
        channel: m.channel,
        kind,
        title: m.title ?? m.channel,
        dmGuest: null,
        character: kind === 'dm' ? character : null,
      })
    }
  }
  return [...map.values()].sort((a, b) => roomOrder(a.kind) - roomOrder(b.kind) || a.title.localeCompare(b.title))
}

function roomsInput(
  channels: ChannelSummary[],
  messages: CockpitMessage[],
  roster: RosterRow[],
  factions: FactionSummary[],
  cast: CastSummary[],
  perspective: string,
): RoomsInput {
  return { channels, messages, roster, factions, cast, perspective }
}

/** The derived rooms list for the current lens, sorted for the sidebar. */
export function useRooms(): Room[] {
  const channels = useCockpit((s) => s.channels)
  const messages = useCockpit((s) => s.messages)
  const roster = useCockpit((s) => s.roster)
  const factions = useCockpit((s) => s.factions)
  const cast = useCockpit((s) => s.cast)
  const perspective = useCockpit((s) => s.perspective)
  return useMemo(
    () => buildRooms(roomsInput(channels, messages, roster, factions, cast, perspective)),
    [channels, messages, roster, factions, cast, perspective],
  )
}

/** Count of lens-visible messages per room key, for the sidebar badges. */
export function useRoomCounts(): Map<string, number> {
  const messages = useCockpit((s) => s.messages)
  const roster = useCockpit((s) => s.roster)
  const cast = useCockpit((s) => s.cast)
  const perspective = useCockpit((s) => s.perspective)
  return useMemo(() => {
    const lens = lensKindOf(perspective, roster, cast)
    const c = new Map<string, number>()
    for (const m of messages) {
      if (!messageInLens(m, perspective, lens)) continue
      const key = roomKeyOf(m)
      c.set(key, (c.get(key) ?? 0) + 1)
    }
    return c
  }, [messages, roster, cast, perspective])
}
