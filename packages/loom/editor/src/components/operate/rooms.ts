//! Shared room model for the Run cockpit. The Rooms rail (left) and the Chat
//! tab (center) both need the same derived list — authored/derived channels
//! plus one room per (character × guest) DM seen in the feed — so it lives here
//! as a hook instead of being duplicated.

import { useMemo } from 'react'
import { useOperate, type OperateMessage } from '@/store/operate'

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

/** The single guest on the far side of a DM message, or null if it isn't a 1:1 DM. */
export function dmGuestOf(m: OperateMessage): string | null {
  const isDm = m.channelKind === 'dm' || m.channel.startsWith('dm:')
  if (isDm && Array.isArray(m.audience) && m.audience.length === 1) return m.audience[0]!
  return null
}

/** Sidebar sort: lobby, factions, announcements, then everything else. */
export function roomOrder(kind: string | undefined): number {
  return kind === 'lobby' ? 0 : kind === 'faction' ? 1 : kind === 'announcement' ? 2 : 3
}

/** The room key a message belongs to (per-guest for DMs, else the channel id). */
export function roomKeyOf(m: OperateMessage): string {
  const gid = dmGuestOf(m)
  return gid ? `${m.channel}#${gid}` : m.channel
}

/** The derived rooms list, sorted for the sidebar. */
export function useRooms(): Room[] {
  const channels = useOperate((s) => s.channels)
  const messages = useOperate((s) => s.messages)
  const roster = useOperate((s) => s.roster)
  return useMemo(() => {
    const nameOf = (gid: string) => roster.find((r) => r.id === gid)?.name ?? gid
    const map = new Map<string, Room>()
    for (const c of channels) {
      map.set(c.id, { key: c.id, channel: c.id, kind: c.kind, title: c.title, dmGuest: null, character: null })
    }
    for (const m of messages) {
      const gid = dmGuestOf(m)
      const character = m.channel.startsWith('dm:') ? m.channel.slice(3) : (m.title ?? m.channel)
      if (gid) {
        const key = `${m.channel}#${gid}`
        if (!map.has(key)) {
          map.set(key, { key, channel: m.channel, kind: 'dm', title: `${character} · ${nameOf(gid)}`, dmGuest: gid, character })
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
  }, [channels, messages, roster])
}

/** Count of messages per room key, for the sidebar badges. */
export function useRoomCounts(): Map<string, number> {
  const messages = useOperate((s) => s.messages)
  return useMemo(() => {
    const c = new Map<string, number>()
    for (const m of messages) {
      const key = roomKeyOf(m)
      c.set(key, (c.get(key) ?? 0) + 1)
    }
    return c
  }, [messages])
}
